use std::io::{Read, Write};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Sesión de shell interactiva sobre un pseudo-terminal.
/// `portable_pty` da una sola implementación para ConPTY (Windows) y PTY (Unix) —
/// evita mantener dos code paths con sus propias rarezas de encoding/resize.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// (sesión, lado de lectura del PTY, lado de escritura del PTY)
pub type SpawnResult = (PtySession, Box<dyn Read + Send>, Box<dyn Write + Send>);

impl PtySession {
    pub fn spawn() -> anyhow::Result<SpawnResult> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = CommandBuilder::new(default_shell());
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
