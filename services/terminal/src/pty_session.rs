use std::io::{Read, Write};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Sesión de shell interactiva sobre un pseudo-terminal.
/// `portable_pty` da una sola implementación para ConPTY (Windows) y PTY (Unix) —
/// evita mantener dos code paths con sus propias rarezas de encoding/resize.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

/// Shells reconocidos — nunca se pasa el string que manda el cliente
/// directo a `CommandBuilder` (sería una vía de RCE si el token de la
/// terminal se filtra alguna vez): `resolve_shell` solo puede devolver uno
/// de estos, todo lo demás cae al default de la plataforma.
#[derive(Clone, Copy)]
pub enum ShellKind {
    PowerShell,
    Cmd,
    Bash,
    Sh,
}

/// Lista de shells disponibles para el SO donde corre el sidecar — la
/// misma que consume `GET /terminal/shells` en `main.rs` para poblar el
/// selector del frontend, así el dropdown nunca ofrece algo que
/// `resolve_shell` no vaya a aceptar.
pub fn available_shells() -> Vec<(&'static str, &'static str)> {
    if cfg!(windows) {
        vec![("powershell", "PowerShell"), ("cmd", "CMD")]
    } else {
        vec![("bash", "Bash"), ("sh", "sh")]
    }
}

/// Resuelve el pedido del cliente (`?shell=...`) contra la allow-list de
/// `available_shells()` — un valor no reconocido, o ausente, degrada al
/// shell default de la plataforma sin error (mismo criterio "degradar con
/// gracia" que el resto del proyecto).
fn resolve_shell(requested: Option<&str>) -> ShellKind {
    match requested {
        Some("powershell") if cfg!(windows) => ShellKind::PowerShell,
        Some("cmd") if cfg!(windows) => ShellKind::Cmd,
        Some("bash") if !cfg!(windows) => ShellKind::Bash,
        Some("sh") if !cfg!(windows) => ShellKind::Sh,
        _ => {
            if cfg!(windows) {
                ShellKind::PowerShell
            } else {
                ShellKind::Bash
            }
        }
    }
}

/// Directorio desde el que arrancó `terminal-server` — la raíz "virtual"
/// del prompt (`$sythrall`). Sythrall se está encaminando a ser un SaaS: el
/// path real del filesystem de la máquina que lo hostea (`C:\Users\PC` en
/// una laptop hoy, algún path de contenedor en la nube mañana) no es
/// información que el usuario final debería ver — el prompt muestra un path
/// relativo a esta raíz (`$sythrall/proyecto`) en vez del absoluto real.
/// Calculado una sola vez: es el mismo para toda sesión que este proceso
/// vaya a spawnear.
fn workspace_root() -> String {
    std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

/// Arma el shell + los argumentos que le dan su prompt de marca desde el
/// arranque — **nunca** por bytes escritos al PTY después de spawnear:
/// probado a mano y produce un "flash" visible, porque el shell todavía no
/// está leyendo stdin cuando esos bytes llegan, así que su line-editor los
/// muestra como si el usuario los hubiera tipeado él mismo (PowerShell:
/// PSReadLine literalmente re-tipea `function prompt {...}` en pantalla
/// antes de aplicarlo). Pasar el prompt como argumento de arranque evita el
/// problema de raíz: el shell nunca llega a mostrar su prompt default.
///
/// El prompt muestra un path virtual relativo a `workspace_root()`
/// (`$sythrall`, `$sythrall/proyecto`, `$sythrall/proyecto/src`), no el
/// path absoluto real — recalculado en cada render del prompt (el shell lo
/// evalúa de nuevo antes de cada línea), así que sigue al `cd` del usuario
/// en vivo. Si el usuario navega FUERA de la raíz (`cd C:\Windows`, `cd ..`
/// más allá del root), degrada a mostrar el path absoluto real con un
/// prefijo `$sythrall:` — mejor eso que mentir sobre dónde está parado.
///
/// Pisa el `PS1`/función `prompt` que el usuario ya tuviera configurado
/// (Oh-My-Posh, etc.) — pero esta es la terminal embebida de Sythrall, no
/// la terminal de sistema del usuario, así que es el comportamiento
/// esperado, no un bug.
///
/// `powershell.exe` acá es Windows PowerShell 5.1, no pwsh 7 — no soporta
/// el escape `` `e `` de PS7+, por eso `[char]27` para el ESC. `cmd.exe` no
/// tiene forma de computar un path relativo dinámicamente (su mini-lenguaje
/// de `prompt` es sustitución de template pura, sin condicionales ni
/// operaciones de string) — se queda con el path absoluto real, mejor eso
/// que un hack frágil. `sh` (a menudo `dash`, no `bash`, en muchas distros)
/// tampoco tiene una forma confiable de inyectar esto sin arriesgar romper
/// su arranque, así que queda sin personalizar. `bash` usa `PROMPT_COMMAND`
/// (se re-evalúa antes de cada prompt, a diferencia de un `PS1` estático) —
/// best-effort, no se pudo probar en la máquina Windows donde se escribió
/// esto.
fn build_command(kind: ShellKind) -> CommandBuilder {
    let root_raw = workspace_root();
    match kind {
        ShellKind::PowerShell => {
            let root = root_raw.replace('\'', "''");
            let template = "function prompt { $e=[char]27; $root='__ROOT__'; $cur=$PWD.Path; if ($cur -eq $root) { $rel='' } elseif ($cur.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar)) { $rel='/' + $cur.Substring($root.Length + 1).Replace('\\','/') } else { $rel=':' + $cur }; \"$e[36m`$sythrall$rel$e[0m> \" }";
            let script = template.replace("__ROOT__", &root);
            let mut cmd = CommandBuilder::new("powershell.exe");
            cmd.args(["-NoLogo", "-NoExit", "-Command", &script]);
            // No alcanza con que `terminal-server` haya arrancado en
            // `root_raw` (heredar el cwd del proceso padre) — probado a
            // mano: ConPTY/PowerShell no lo respeta de forma confiable acá,
            // así que se fija explícito para que `$PWD.Path` arranque
            // exactamente en `root_raw`, matcheando `$root` desde el primer
            // prompt (si no, cae siempre a la rama ":ruta absoluta").
            cmd.cwd(&root_raw);
            cmd
        }
        ShellKind::Cmd => {
            let mut cmd = CommandBuilder::new("cmd.exe");
            cmd.args(["/K", "prompt Sythrall $P$G"]);
            cmd.cwd(&root_raw);
            cmd
        }
        ShellKind::Bash => {
            let root = root_raw.replace('\'', "'\\''");
            let template = "case \"$PWD\" in '__ROOT__') PS1='\\[\\e[36m\\]$sythrall\\[\\e[0m\\]> ' ;; '__ROOT__'/*) PS1=\"\\[\\e[36m\\]\\$sythrall/${PWD#'__ROOT__'/}\\[\\e[0m\\]> \" ;; *) PS1=\"\\[\\e[36m\\]\\$sythrall:$PWD\\[\\e[0m\\]> \" ;; esac";
            let prompt_command = template.replace("__ROOT__", &root);
            let mut cmd = CommandBuilder::new("/bin/bash");
            cmd.env("PROMPT_COMMAND", prompt_command);
            cmd.cwd(&root_raw);
            cmd
        }
        ShellKind::Sh => {
            let mut cmd = CommandBuilder::new("/bin/sh");
            cmd.cwd(&root_raw);
            cmd
        }
    }
}

/// (sesión, lado de lectura del PTY, lado de escritura del PTY)
pub type SpawnResult = (PtySession, Box<dyn Read + Send>, Box<dyn Write + Send>);

impl PtySession {
    pub fn spawn(requested_shell: Option<&str>) -> anyhow::Result<SpawnResult> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let kind = resolve_shell(requested_shell);
        let cmd = build_command(kind);
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        Ok((
            Self {
                master: pair.master,
                child,
            },
            reader,
            writer,
        ))
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}
