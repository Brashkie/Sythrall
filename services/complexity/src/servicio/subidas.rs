//! **Proyectos subidos — Fase 35, portado desde `routers/upload.py` y
//! `services/project_service.py`.**
//!
//! El árbol de archivos, la metadata cacheada de cada proyecto, la extracción
//! de ZIPs, la poda de los viejos y los chequeos de path traversal. Es I/O y
//! lógica de producto, no análisis: ningún parser se toca acá.
//!
//! **Lo que el port encontró y no estaba en el código viejo** está marcado en
//! cada sitio, pero el más importante va acá arriba porque afecta qué archivos
//! termina viendo el analizador: **había DOS listas de directorios ignorados
//! distintas** en el mismo producto —la de `project_service.py` y la de
//! `servicio/proyecto.rs`— y no coincidían. Ver `DIRECTORIOS_IGNORADOS`.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::servicio::logstore;

/// El archivo donde cada proyecto cachea su metadata.
pub const ARCHIVO_DE_METADATA: &str = ".sythrall-meta.json";

/// 50 MB por archivo, igual que antes.
pub const MAX_ARCHIVO: u64 = 50 * 1024 * 1024;
/// 200 MB por ZIP.
pub const MAX_ZIP: u64 = 200 * 1024 * 1024;
/// 5 MB para leer un archivo y devolver su texto.
pub const MAX_LECTURA: u64 = 5 * 1024 * 1024;
/// Cuántos proyectos se conservan antes de podar los más viejos.
pub const MAX_PROYECTOS: usize = 20;

/// Extensiones que no se escriben a disco, vengan de donde vengan.
pub const EXTENSIONES_BLOQUEADAS: &[&str] =
    &[".exe", ".dll", ".so", ".bin", ".sh", ".bat", ".cmd", ".ps1"];

/// **Una sola lista de directorios ignorados, porque había dos y no
/// coincidían.**
///
/// `project_service.py` (subidas) ignoraba `.tox` y `*.egg-info`;
/// `proyecto.rs` (análisis) ignoraba `target` y `.cache`. Resultado: subir una
/// carpeta de un proyecto Rust escribía todo `target/` a disco —y en un build
/// real son decenas de miles de archivos— y recién el analizador los
/// descartaba después. Y al revés, un `.tox` sí llegaba al análisis.
///
/// Esta es la **unión** de las dos, con una corrección: `"*.egg-info"` estaba
/// en un set que se consultaba por igualdad exacta de nombre, así que **nunca
/// matcheaba nada** — era código muerto que parecía una regla. Lo que la
/// intención decía se implementa de verdad en `es_ignorado`, por sufijo.
pub const DIRECTORIOS_IGNORADOS: &[&str] = &[
    "__pycache__",
    ".git",
    ".svn",
    "node_modules",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "target",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    ".cache",
];

/// `true` si este segmento de ruta es un directorio que no se analiza ni se
/// escribe. El sufijo `.egg-info` va por separado porque es un patrón, no un
/// nombre: `mipaquete.egg-info` tiene que entrar y una lista de nombres
/// exactos no lo agarra.
pub fn es_ignorado(segmento: &str) -> bool {
    DIRECTORIOS_IGNORADOS.contains(&segmento) || segmento.ends_with(".egg-info")
}

/// Extensiones que cuentan como código para las estadísticas del proyecto.
pub const EXTENSIONES_DE_CODIGO: &[&str] = &[
    ".py", ".ts", ".tsx", ".js", ".jsx", ".java", ".cpp", ".c", ".cs", ".go", ".rs", ".rb", ".php",
    ".swift", ".kt", ".scala", ".r", ".html", ".css", ".scss", ".sass", ".less", ".vue", ".svelte",
    ".json", ".yaml", ".yml", ".toml", ".ini", ".env", ".md", ".txt", ".sql", ".graphql", ".sh",
    ".bash", ".zsh", ".f", ".f90", ".f95", ".f03", ".f08", ".for", ".adb", ".d", ".ads", ".s",
    ".asm", ".ipynb",
];

/// La extensión en minúsculas **con el punto**, o cadena vacía si no tiene.
///
/// No usa `Path::extension` tal cual: ése devuelve `"py"` y toda esta lógica
/// —las dos listas de arriba y el shape que el frontend consume— está escrita
/// con punto, como `Path.suffix` de Python.
pub fn extension_de(nombre: &str) -> String {
    match nombre.rfind('.') {
        // Un nombre que arranca con punto y no tiene otro (`.gitignore`) NO
        // tiene extensión: es el mismo criterio que `Path.suffix`, y tratarlo
        // como extensión `.gitignore` rompería los dos sets de arriba.
        Some(0) | None => String::new(),
        Some(i) => nombre[i..].to_ascii_lowercase(),
    }
}

pub fn es_codigo(ext: &str) -> bool {
    EXTENSIONES_DE_CODIGO.contains(&ext)
}

pub fn esta_bloqueada(ext: &str) -> bool {
    EXTENSIONES_BLOQUEADAS.contains(&ext)
}

/// `1.5 KB` — mismo formato que `_fmt_size`.
pub fn formato_de_tamano(bytes: u64) -> String {
    let mut n = bytes as f64;
    for unidad in ["B", "KB", "MB", "GB"] {
        if n < 1024.0 {
            return format!("{n:.1} {unidad}");
        }
        n /= 1024.0;
    }
    format!("{n:.1} TB")
}

/// Un UUID v4 en el formato canónico.
///
/// Se arma con `rand`, que ya era dependencia, en vez de agregar una crate de
/// UUID: lo único que se necesita es un identificador aleatorio con la forma
/// que el frontend y las URLs ya usan. Los bits de versión y variante se
/// fijan igual que `uuid.uuid4()` de Python para que un id nuevo no se
/// distinga de uno viejo.
pub fn uuid_v4() -> String {
    use rand::Rng;
    let mut b: [u8; 16] = rand::thread_rng().gen();
    b[6] = (b[6] & 0x0f) | 0x40; // versión 4
    b[8] = (b[8] & 0x3f) | 0x80; // variante RFC 4122
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
}

// ── Path traversal ────────────────────────────────────────────────────────

/// El directorio de un proyecto, **sólo si queda dentro del de subidas**.
///
/// Compara rutas canonicalizadas en vez de prefijos de texto. El chequeo
/// viejo usaba `startswith`, y un directorio hermano cuyo nombre empieza
/// igual (`uploads/projects-backup` contra `uploads/projects`) lo pasaba
/// estando completamente afuera — era un traversal real, no teórico.
pub fn ruta_de_proyecto(project_id: &str) -> Option<PathBuf> {
    let base = directorio_de_subidas();
    let candidato = base.join(project_id);
    // `canonicalize` falla si el path no existe, así que se normaliza a mano:
    // esto tiene que rechazar `../..` ANTES de saber si existe.
    let normalizado = normalizar(&candidato);
    let base_normalizada = normalizar(&base);
    if normalizado.starts_with(&base_normalizada) && normalizado != base_normalizada {
        Some(normalizado)
    } else {
        None
    }
}

/// Resuelve `.` y `..` sin tocar el disco. Trabaja sobre componentes, así que
/// no se puede engañar con separadores repetidos ni con un `..` de más.
pub fn normalizar(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            otro => out.push(otro.as_os_str()),
        }
    }
    out
}

/// Los segmentos de una ruta relativa, ya saneados: sin `.`, sin `..`, sin
/// vacíos, y sin raíz ni letra de unidad. Vacío si no queda nada usable.
pub fn segmentos_seguros(ruta: &str) -> Vec<String> {
    ruta.split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        // Un segmento como `C:` sólo aparece si el cliente mandó una ruta
        // absoluta de Windows; se descarta junto con el resto de la basura.
        .filter(|s| !s.ends_with(':'))
        .map(str::to_string)
        .collect()
}

pub fn directorio_de_subidas() -> PathBuf {
    crate::servicio::proyecto::directorio_de_subidas()
}

// ── Área temporal de subida ───────────────────────────────────────────────

/// Un directorio de staging para una subida en curso.
///
/// **Existe por una diferencia concreta entre FastAPI y axum.** FastAPI
/// parseaba el `multipart/form-data` entero antes de correr el handler, así
/// que `project_id` ya estaba disponible cuando empezaba a escribir archivos.
/// En axum el multipart es un **stream**: los campos llegan en el orden en
/// que el cliente los puso, y `project_id` puede venir *después* de los
/// archivos — no se sabe a qué directorio van hasta que termina el cuerpo.
///
/// Las dos salidas eran acumular la subida entera en RAM (lo que FastAPI
/// hacía, vía `SpooledTemporaryFile`) o escribir a un temporal y mover al
/// final. Se eligió el temporal: **la RAM queda constante en un archivo por
/// vez** sin importar cuántos traiga la subida, que es exactamente el eje que
/// la fase pide mejorar. El caso real que motivó el filtro de `node_modules`
/// eran 27.614 archivos y 513 MB; ese cuerpo ya no tiene que caber en memoria.
///
/// Va **afuera** del directorio que se lista: dentro, `listar()` y `podar()`
/// verían una subida a medio hacer como un proyecto más.
pub fn directorio_temporal_de_subida() -> PathBuf {
    let base = directorio_de_subidas();
    let padre = base.parent().map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
    padre.join(".subidas-en-curso").join(uuid_v4())
}

/// Mueve todo el contenido de `desde` dentro de `hasta`, fusionando
/// directorios que ya existan.
///
/// Intenta `rename` por archivo primero —en el mismo volumen es un cambio de
/// metadata, sin copiar bytes— y cae a copiar sólo si falla, que es lo que
/// pasa si el temporal y el destino quedaron en volúmenes distintos.
pub fn mover_contenido(desde: &Path, hasta: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(hasta)?;
    for entrada in std::fs::read_dir(desde)?.flatten() {
        let origen = entrada.path();
        let destino = hasta.join(entrada.file_name());
        if origen.is_dir() {
            mover_contenido(&origen, &destino)?;
            let _ = std::fs::remove_dir(&origen);
        } else if std::fs::rename(&origen, &destino).is_err() {
            std::fs::copy(&origen, &destino)?;
            let _ = std::fs::remove_file(&origen);
        }
    }
    Ok(())
}

/// Las rutas relativas de todos los archivos de un árbol, en POSIX.
pub fn rutas_relativas(dir: &Path) -> Vec<String> {
    recorrer(dir).iter().map(|(p, _)| ruta_relativa(p, dir)).collect()
}

// ── Árbol de archivos ─────────────────────────────────────────────────────

pub const PROFUNDIDAD_MAXIMA: usize = 10;

/// El árbol de archivos de un proyecto, con el shape que el panel consume.
pub fn arbol(dir: &Path) -> Value {
    arbol_desde(dir, dir, 0)
}

fn ruta_relativa(p: &Path, raiz: &Path) -> String {
    match p.strip_prefix(raiz) {
        // `.as_posix()` y no el separador nativo: el frontend hace
        // `path.split('/')` para sacar el nombre del archivo, así que un `\`
        // de Windows le devolvía la ruta entera como "nombre" — y además
        // sería inconsistente con el mismo backend corriendo en Linux.
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
        Err(_) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
    }
}

fn arbol_desde(dir: &Path, raiz: &Path, profundidad: usize) -> Value {
    let nombre = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // El nodo raíz es el único cuyo `path` es su propio nombre: no hay nada
    // "relativo a sí mismo" que tenga sentido.
    let propia = if dir == raiz { nombre.clone() } else { ruta_relativa(dir, raiz) };

    if profundidad > PROFUNDIDAD_MAXIMA {
        return json!({
            "name": nombre, "type": "directory", "path": propia,
            "children": [], "truncated": true,
        });
    }

    let Ok(entradas) = std::fs::read_dir(dir) else {
        return json!({
            "name": nombre, "type": "directory", "path": propia,
            "children": [], "error": "Sin permisos de lectura",
        });
    };

    let mut directorios: Vec<PathBuf> = Vec::new();
    let mut archivos: Vec<PathBuf> = Vec::new();
    for e in entradas.flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !es_ignorado(&n) {
                directorios.push(p);
            }
        } else if n != ARCHIVO_DE_METADATA {
            archivos.push(p);
        }
    }
    // Directorios primero y después archivos, cada grupo alfabético sin
    // distinguir mayúsculas — el mismo orden que daba la clave
    // `(p.is_file(), p.name.lower())`.
    let clave = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    directorios.sort_by_key(clave);
    archivos.sort_by_key(clave);

    let mut hijos: Vec<Value> = directorios
        .iter()
        .map(|d| arbol_desde(d, raiz, profundidad + 1))
        .collect();

    for f in &archivos {
        let nombre = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let ext = extension_de(&nombre);
        let meta = f.metadata().ok();
        let tamano = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modificado = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| logstore::iso_de_segundos(d.as_secs()))
            .unwrap_or_default();
        hijos.push(json!({
            "name": nombre,
            "type": "file",
            "path": ruta_relativa(f, raiz),
            "size": tamano,
            "size_fmt": formato_de_tamano(tamano),
            "extension": ext,
            "is_code": es_codigo(&ext),
            "modified": modificado,
        }));
    }

    json!({ "name": nombre, "type": "directory", "path": propia, "children": hijos })
}

// ── Metadata del proyecto ─────────────────────────────────────────────────

/// Recorre el proyecto y calcula sus estadísticas. **Toca disco entero**: se
/// llama una vez al subir, y para leer después está `info_cacheada`.
pub fn info_de(dir: &Path) -> Value {
    let mut total = 0u64;
    let mut bytes = 0u64;
    let mut de_codigo = 0u64;
    let mut por_extension: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();

    for (ruta, meta) in recorrer(dir) {
        let nombre = ruta.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if nombre == ARCHIVO_DE_METADATA {
            continue;
        }
        total += 1;
        bytes += meta.len();
        let ext = extension_de(&nombre);
        let clave = if ext.is_empty() { "(sin extensión)".to_string() } else { ext.clone() };
        *por_extension.entry(clave).or_insert(0) += 1;
        if es_codigo(&ext) {
            de_codigo += 1;
        }
    }

    // Las 10 extensiones más frecuentes. El desempate va por nombre para que
    // dos corridas sobre el mismo proyecto den el mismo orden: con `sorted`
    // por conteo nada más, dos extensiones empatadas podían alternarse entre
    // llamadas y el panel mostraba un orden distinto sin que nada cambiara.
    let mut pares: Vec<(String, u64)> = por_extension.into_iter().collect();
    pares.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut top = Map::new();
    for (k, v) in pares.into_iter().take(10) {
        top.insert(k, json!(v));
    }

    json!({
        "total_files": total,
        "total_size": bytes,
        "total_size_fmt": formato_de_tamano(bytes),
        "code_files": de_codigo,
        "by_extension": top,
        "created_at": logstore::iso_de_segundos(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        ),
    })
}

/// Todos los archivos de un árbol, sin entrar en los directorios ignorados.
///
/// Iterativo y no recursivo: un proyecto subido puede traer anidamientos
/// arbitrarios, y una recursión sin tope revienta la pila con un árbol
/// suficientemente profundo.
fn recorrer(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    let mut out = Vec::new();
    let mut pendientes = vec![dir.to_path_buf()];
    while let Some(actual) = pendientes.pop() {
        let Ok(entradas) = std::fs::read_dir(&actual) else { continue };
        for e in entradas.flatten() {
            let p = e.path();
            let n = e.file_name().to_string_lossy().to_string();
            match e.metadata() {
                Ok(m) if m.is_dir() => {
                    if !es_ignorado(&n) {
                        pendientes.push(p);
                    }
                }
                Ok(m) => out.push((p, m)),
                Err(_) => {}
            }
        }
    }
    out
}

pub fn guardar_metadata(dir: &Path, info: &Value) {
    // Best-effort, igual que antes: si no se puede cachear, se recalcula la
    // próxima vez. No es motivo para fallar una subida que ya se escribió.
    let _ = std::fs::write(
        dir.join(ARCHIVO_DE_METADATA),
        serde_json::to_string(info).unwrap_or_else(|_| "{}".to_string()),
    );
}

fn metadata_guardada(dir: &Path) -> Option<Value> {
    std::fs::read_to_string(dir.join(ARCHIVO_DE_METADATA))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// Recalcula la metadata de un proyecto **conservando su nombre** y la guarda.
///
/// **Esto arregla un bug que venía de Python.** `create_project_file` hacía
/// `get_project_info()` + `save_project_meta()`, y `get_project_info` no
/// devuelve `project_name`: el nombre se escribía sólo en el camino de
/// creación (`resolve_project_name`), así que **cada archivo nuevo que se
/// creaba desde el explorador le borraba el nombre al proyecto** y la lista
/// de proyectos recientes lo mostraba sin él a partir de ahí. Se portó el
/// comportamiento y se vio en la verificación real: crear `src/main.py` en un
/// proyecto llamado "desde-cero" dejaba `project_name: null`.
///
/// Si el proyecto no tenía nombre guardado, no se le inventa uno: la clave
/// simplemente no se agrega, que es distinto de ponerle cadena vacía.
pub fn actualizar_metadata(dir: &Path) -> Value {
    let previo = metadata_guardada(dir)
        .and_then(|v| v.get("project_name").and_then(Value::as_str).map(str::to_string));
    let mut info = info_de(dir);
    if let (Some(nombre), Some(o)) = (previo, info.as_object_mut()) {
        o.insert("project_name".to_string(), json!(nombre));
    }
    guardar_metadata(dir, &info);
    info
}

/// La metadata cacheada, o la calculada y guardada si no había.
pub fn info_cacheada(dir: &Path) -> Value {
    if let Some(v) = metadata_guardada(dir) {
        return v;
    }
    let info = info_de(dir);
    guardar_metadata(dir, &info);
    info
}

/// Decide el nombre final del proyecto y lo escribe dentro de `info`, que el
/// caller todavía tiene que persistir.
///
/// **Si el proyecto ya existía y no vino un nombre nuevo, se conserva el que
/// tenía** en vez de pisarlo con el fallback genérico basado en el id: sumar
/// archivos a un proyecto no es renombrarlo.
pub fn resolver_nombre(dir: &Path, info: &mut Value, pedido: &str, fallback: &str) -> String {
    let previo = metadata_guardada(dir)
        .and_then(|v| v.get("project_name").and_then(Value::as_str).map(str::to_string))
        .filter(|s| !s.is_empty());
    let resuelto = if !pedido.is_empty() {
        pedido.to_string()
    } else {
        previo.unwrap_or_else(|| fallback.to_string())
    };
    if let Some(o) = info.as_object_mut() {
        o.insert("project_name".to_string(), json!(resuelto));
    }
    resuelto
}

// ── Listar, borrar, podar ─────────────────────────────────────────────────

/// Los proyectos en disco, el más reciente primero, desde la metadata
/// cacheada — **sin recorrer el árbol de cada uno**, que con muchos proyectos
/// acumulados bloqueaba el servidor en cada llamada.
pub fn listar() -> Vec<Value> {
    let base = directorio_de_subidas();
    let Ok(entradas) = std::fs::read_dir(&base) else { return Vec::new() };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entradas
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let t = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (t, e.path())
        })
        .collect();
    dirs.sort_by_key(|(t, _)| std::cmp::Reverse(*t));

    dirs.iter()
        .map(|(_, p)| {
            let mut v = info_cacheada(p);
            if let Some(o) = v.as_object_mut() {
                o.insert(
                    "project_id".to_string(),
                    json!(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                );
                o.insert("path".to_string(), json!(p.to_string_lossy()));
            }
            v
        })
        .collect()
}

pub fn borrar_proyecto(dir: &Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(dir)
}

/// Borra los proyectos más viejos cuando hay más de `conservar`. Devuelve
/// cuántos se borraron.
///
/// **Esto es lo que borró 20 proyectos reales durante una corrida de tests**,
/// porque la suite de Python escribía en el directorio de verdad. El arreglo
/// no fue acá —la poda hace lo que dice— sino en `conftest.py`, apuntando los
/// tests a un temporal. Queda anotado donde se ve el efecto.
pub fn podar(conservar: usize) -> usize {
    let base = directorio_de_subidas();
    let Ok(entradas) = std::fs::read_dir(&base) else { return 0 };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entradas
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            (t, e.path())
        })
        .collect();
    if dirs.len() <= conservar {
        return 0;
    }
    dirs.sort_by_key(|(t, _)| std::cmp::Reverse(*t)); // más nuevo primero
    let mut borrados = 0;
    for (_, p) in dirs.iter().skip(conservar) {
        if borrar_proyecto(p).is_ok() {
            borrados += 1;
        }
    }
    borrados
}

// ── Extracción de ZIP ─────────────────────────────────────────────────────

/// Descomprime un ZIP dentro de `destino`, con los mismos chequeos que antes.
///
/// Devuelve `{extracted, skipped, errors}`. **Un archivo que falla no aborta
/// la extracción**: se anota en `errors` y sigue, porque un ZIP con un archivo
/// raro sigue siendo útil por los otros 300.
pub fn extraer_zip(bytes: &[u8], destino: &Path) -> Value {
    let mut extraidos = 0u64;
    let mut salteados = 0u64;
    let mut errores: Vec<Value> = Vec::new();

    let entradas = match crate::servicio::zip::entradas(bytes) {
        Ok(e) => e,
        Err(e) => {
            return json!({ "extracted": 0, "skipped": 0, "errors": [{"file": "", "reason": e}] });
        }
    };

    for entrada in &entradas {
        // Un directorio no tiene contenido que escribir; los padres se crean
        // al escribir cada archivo.
        if entrada.nombre.ends_with('/') {
            continue;
        }
        let partes = segmentos_seguros(&entrada.nombre);
        if partes.is_empty() {
            salteados += 1;
            continue;
        }
        let ext = extension_de(partes.last().expect("no vacío"));
        if esta_bloqueada(&ext) {
            errores.push(json!({ "file": entrada.nombre, "reason": format!("Extensión bloqueada: {ext}") }));
            salteados += 1;
            continue;
        }
        if partes[..partes.len() - 1].iter().any(|p| es_ignorado(p)) {
            salteados += 1;
            continue;
        }
        // **El tamaño se chequea contra el declarado ANTES de inflar.** Es la
        // defensa contra una bomba de descompresión: un ZIP de pocos KB puede
        // declarar gigabytes, y descomprimirlo para después medirlo sería
        // llenar la RAM antes de poder rechazarlo.
        if entrada.tamano_real > MAX_ARCHIVO {
            errores.push(json!({
                "file": entrada.nombre,
                "reason": "Archivo muy grande (>50 MB descomprimido)"
            }));
            salteados += 1;
            continue;
        }

        match crate::servicio::zip::leer(bytes, entrada) {
            Ok(datos) => {
                // Y se vuelve a chequear sobre lo que REALMENTE salió: el
                // tamaño del directorio central es un dato del archivo, o sea
                // de quien lo armó, y puede mentir.
                if datos.len() as u64 > MAX_ARCHIVO {
                    errores.push(json!({
                        "file": entrada.nombre,
                        "reason": "Archivo muy grande (>50 MB descomprimido)"
                    }));
                    salteados += 1;
                    continue;
                }
                let destino_final = partes.iter().fold(destino.to_path_buf(), |a, p| a.join(p));
                if let Some(padre) = destino_final.parent() {
                    let _ = std::fs::create_dir_all(padre);
                }
                match std::fs::write(&destino_final, &datos) {
                    Ok(()) => extraidos += 1,
                    Err(e) => errores.push(json!({ "file": entrada.nombre, "reason": e.to_string() })),
                }
            }
            Err(e) => errores.push(json!({ "file": entrada.nombre, "reason": e })),
        }
    }

    json!({ "extracted": extraidos, "skipped": salteados, "errors": errores })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporal(sufijo: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "subidas-{}-{}-{:?}",
            std::process::id(),
            sufijo,
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn escribir(dir: &Path, rel: &str, contenido: &str) {
        let p = rel.split('/').fold(dir.to_path_buf(), |a, s| a.join(s));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contenido).unwrap();
    }

    #[test]
    fn la_extension_lleva_el_punto_y_va_en_minuscula() {
        assert_eq!(extension_de("App.PY"), ".py");
        assert_eq!(extension_de("a.tar.gz"), ".gz");
        assert_eq!(extension_de("Makefile"), "");
    }

    /// **Un dotfile no tiene extensión.** `.gitignore` tratado como extensión
    /// `.gitignore` no estaría en ningún set y además rompería el agrupamiento
    /// por extensión del panel.
    #[test]
    fn un_dotfile_no_tiene_extension() {
        assert_eq!(extension_de(".gitignore"), "");
        assert_eq!(extension_de(".env"), "");
        // Pero sí la tiene si hay un punto más adentro.
        assert_eq!(extension_de(".eslintrc.json"), ".json");
    }

    /// **`*.egg-info` nunca matcheaba.** Estaba en un set consultado por
    /// igualdad exacta, así que era una regla que parecía existir y no hacía
    /// nada. Ahora se implementa por sufijo, que es lo que decía la intención.
    #[test]
    fn el_patron_egg_info_ahora_si_matchea() {
        assert!(es_ignorado("mipaquete.egg-info"));
        assert!(es_ignorado("otro_paquete.egg-info"));
        assert!(es_ignorado("node_modules"));
        assert!(!es_ignorado("src"));
    }

    /// **Las dos listas que había no coincidían**: `target` sólo estaba en la
    /// del análisis y `.tox` sólo en la de subidas. La unificada tiene las dos.
    #[test]
    fn la_lista_unificada_tiene_lo_que_cada_mitad_le_faltaba() {
        assert!(es_ignorado("target"), "faltaba del lado de las subidas");
        assert!(es_ignorado(".tox"), "faltaba del lado del analisis");
        assert!(es_ignorado(".cache"));
    }

    /// El traversal se rechaza **antes** de saber si el path existe.
    #[test]
    fn un_project_id_con_traversal_se_rechaza() {
        assert!(ruta_de_proyecto("../../etc/passwd").is_none());
        assert!(ruta_de_proyecto("..").is_none());
        assert!(ruta_de_proyecto("").is_none(), "vacio seria el directorio base mismo");
        assert!(ruta_de_proyecto("un-id-normal").is_some());
    }

    /// **El bug del `startswith`**: un hermano cuyo nombre empieza igual
    /// quedaba adentro aunque esté completamente afuera.
    #[test]
    fn un_hermano_con_prefijo_parecido_no_pasa() {
        // `uploads/projects` + `../projects-backup/x` sale del base.
        assert!(ruta_de_proyecto("../projects-backup/x").is_none());
    }

    #[test]
    fn los_segmentos_se_sanean() {
        assert_eq!(segmentos_seguros("src/app.ts"), vec!["src", "app.ts"]);
        assert_eq!(segmentos_seguros("../../etc/passwd"), vec!["etc", "passwd"]);
        assert_eq!(segmentos_seguros("./a//b/"), vec!["a", "b"]);
        assert!(segmentos_seguros("../..").is_empty());
        assert_eq!(segmentos_seguros("C:\\Windows\\x.dll"), vec!["Windows", "x.dll"]);
    }

    #[test]
    fn un_uuid_tiene_la_forma_canonica_y_version_4() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        let partes: Vec<&str> = u.split('-').collect();
        assert_eq!(partes.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        assert!(u.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_eq!(&partes[2][0..1], "4", "el nibble de version tiene que ser 4");
        assert!(matches!(&partes[3][0..1], "8" | "9" | "a" | "b"), "variante RFC 4122");
        assert_ne!(uuid_v4(), uuid_v4(), "dos llamadas no pueden dar lo mismo");
    }

    #[test]
    fn el_formato_de_tamano_sube_de_unidad() {
        assert_eq!(formato_de_tamano(0), "0.0 B");
        assert_eq!(formato_de_tamano(1023), "1023.0 B");
        assert_eq!(formato_de_tamano(1024), "1.0 KB");
        assert_eq!(formato_de_tamano(1024 * 1024 * 3), "3.0 MB");
    }

    /// **El `path` de cada archivo es relativo a la RAÍZ del proyecto, no al
    /// padre inmediato.** El frontend lo usa tal cual para pedir el contenido,
    /// así que un `src/main.py` reportado como `main.py` daba 404 al abrirlo.
    #[test]
    fn el_path_del_arbol_es_relativo_a_la_raiz() {
        let d = temporal("arbol");
        escribir(&d, "raiz.py", "x = 1\n");
        escribir(&d, "src/main.py", "y = 2\n");
        escribir(&d, "src/util/helpers.py", "z = 3\n");
        let t = arbol(&d);

        fn buscar<'a>(nodo: &'a Value, nombre: &str) -> Option<&'a Value> {
            if nodo["name"] == nombre {
                return Some(nodo);
            }
            nodo["children"].as_array()?.iter().find_map(|h| buscar(h, nombre))
        }
        assert_eq!(buscar(&t, "main.py").unwrap()["path"], "src/main.py");
        assert_eq!(buscar(&t, "helpers.py").unwrap()["path"], "src/util/helpers.py");
        assert_eq!(buscar(&t, "raiz.py").unwrap()["path"], "raiz.py");
        // Con separadores POSIX incluso en Windows.
        assert!(!buscar(&t, "main.py").unwrap()["path"].as_str().unwrap().contains('\\'));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn el_arbol_pone_directorios_primero_y_oculta_la_metadata() {
        let d = temporal("orden");
        escribir(&d, "zeta.py", "");
        escribir(&d, "alfa/x.py", "");
        std::fs::write(d.join(ARCHIVO_DE_METADATA), "{}").unwrap();
        escribir(&d, "node_modules/paquete/index.js", "");
        let t = arbol(&d);
        let nombres: Vec<&str> =
            t["children"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert_eq!(nombres, vec!["alfa", "zeta.py"], "directorios primero, sin meta ni node_modules");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_info_cuenta_codigo_y_no_cuenta_la_metadata() {
        let d = temporal("info");
        escribir(&d, "a.py", "x = 1\n");
        escribir(&d, "b.rs", "fn main() {}\n");
        escribir(&d, "notas.bin", "no es codigo");
        std::fs::write(d.join(ARCHIVO_DE_METADATA), "{}").unwrap();
        let i = info_de(&d);
        assert_eq!(i["total_files"], 3, "la metadata no cuenta");
        assert_eq!(i["code_files"], 2, "solo .py y .rs");
        assert!(i["by_extension"][".py"] == 1 && i["by_extension"][".rs"] == 1);
        assert!(i["created_at"].as_str().unwrap().contains('T'), "ISO con T");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_info_no_entra_en_los_directorios_ignorados() {
        let d = temporal("ignorados");
        escribir(&d, "a.py", "x\n");
        escribir(&d, "target/debug/enorme.rs", "basura\n");
        escribir(&d, "node_modules/p/i.js", "basura\n");
        escribir(&d, "mipkg.egg-info/PKG-INFO", "basura\n");
        let i = info_de(&d);
        assert_eq!(i["total_files"], 1, "solo a.py; target/node_modules/egg-info quedan afuera");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **Sumar archivos a un proyecto no es renombrarlo.**
    #[test]
    fn el_nombre_ya_guardado_se_conserva_si_no_viene_uno_nuevo() {
        let d = temporal("nombre");
        let mut i = json!({});
        assert_eq!(resolver_nombre(&d, &mut i, "", "fallback-1"), "fallback-1");
        guardar_metadata(&d, &i);

        let mut i2 = json!({});
        assert_eq!(
            resolver_nombre(&d, &mut i2, "", "fallback-2"),
            "fallback-1",
            "se conserva el guardado, no el fallback nuevo"
        );
        let mut i3 = json!({});
        assert_eq!(resolver_nombre(&d, &mut i3, "pedido", "fallback-3"), "pedido", "un pedido explicito gana");
        assert_eq!(i3["project_name"], "pedido", "y queda escrito en info");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **Recalcular la metadata no le borra el nombre al proyecto.**
    ///
    /// Era un bug real heredado de Python: crear un archivo nuevo desde el
    /// explorador llamaba `get_project_info` + `save_project_meta`, y el
    /// primero no devuelve `project_name`, así que el nombre se perdía y la
    /// lista de proyectos recientes lo mostraba sin él de ahí en adelante.
    #[test]
    fn recalcular_la_metadata_conserva_el_nombre() {
        let d = temporal("meta-nombre");
        let mut info = json!({});
        resolver_nombre(&d, &mut info, "mi-proyecto", "fallback");
        guardar_metadata(&d, &info);
        escribir(&d, "src/main.py", "x = 1\n");

        let nueva = actualizar_metadata(&d);
        assert_eq!(nueva["project_name"], "mi-proyecto", "el nombre tiene que sobrevivir");
        assert_eq!(nueva["total_files"], 1, "y el conteo tiene que estar recalculado");
        // Y en disco, no sólo en el valor devuelto.
        assert_eq!(info_cacheada(&d)["project_name"], "mi-proyecto");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Un proyecto que nunca tuvo nombre no recibe uno inventado: la clave
    /// ausente y la cadena vacía no son lo mismo para el panel.
    #[test]
    fn un_proyecto_sin_nombre_no_recibe_uno_inventado() {
        let d = temporal("meta-sin-nombre");
        escribir(&d, "a.py", "x\n");
        let nueva = actualizar_metadata(&d);
        assert!(nueva.get("project_name").is_none(), "no se inventa un nombre");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **El tamaño declarado se chequea ANTES de inflar**: es la defensa
    /// contra una bomba de descompresión. Un ZIP de pocos bytes que declara
    /// 60 MB se rechaza sin gastar un solo byte de RAM en inflarlo.
    ///
    /// Se verifica sobre el camino real: se arma un ZIP y se le parchea el
    /// tamaño descomprimido en el directorio central, que es de donde sale el
    /// dato — exactamente lo que haría un archivo malicioso.
    #[test]
    fn una_entrada_que_declara_mas_del_maximo_se_rechaza_sin_inflarla() {
        let d = temporal("bomba");
        let mut z = zip_de(&[("enorme.txt", b"chiquito".as_slice())]);

        // El offset del central está guardado en el EOCD (últimos 22 bytes,
        // sin comentario): campo `offset del inicio del central` en +16.
        let eocd = z.len() - 22;
        let inicio_central =
            u32::from_le_bytes(z[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        // En el header central: +24 es el tamaño descomprimido.
        let declarado = inicio_central + 24;
        z[declarado..declarado + 4].copy_from_slice(&(60u32 * 1024 * 1024).to_le_bytes());

        let r = extraer_zip(&z, &d);
        assert_eq!(r["extracted"], 0, "no se tiene que escribir nada");
        assert_eq!(r["skipped"], 1);
        assert!(
            r["errors"][0]["reason"].as_str().unwrap().contains("muy grande"),
            "y tiene que decir por qué: {:?}",
            r["errors"]
        );
        assert!(!d.join("enorme.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn extraer_un_zip_escribe_los_archivos_y_saltea_lo_que_debe() {
        let d = temporal("zip");
        // Un ZIP con: un archivo normal, uno en node_modules, uno con
        // extensión bloqueada y uno con traversal.
        let z = zip_de(&[
            ("src/app.py", b"print(1)\n".as_slice()),
            ("node_modules/p/i.js", b"x\n"),
            ("virus.exe", b"MZ"),
            ("../escape.txt", b"no"),
        ]);
        let r = extraer_zip(&z, &d);
        assert_eq!(r["extracted"], 2, "app.py y escape.txt (saneado a escape.txt)");
        assert!(d.join("src").join("app.py").is_file());
        assert!(!d.join("node_modules").exists(), "node_modules no se escribe");
        assert!(!d.join("virus.exe").exists(), "la extension bloqueada no se escribe");
        // El traversal se sanea, no se escribe afuera.
        assert!(d.join("escape.txt").is_file());
        assert!(!d.parent().unwrap().join("escape.txt").exists(), "NO tiene que salir del destino");
        let razones: Vec<String> = r["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["reason"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(razones.iter().any(|x| x.contains("bloqueada")), "el .exe se reporta: {razones:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_zip_invalido_devuelve_el_error_y_no_panifica() {
        let d = temporal("zip-malo");
        let r = extraer_zip(b"esto no es un zip", &d);
        assert_eq!(r["extracted"], 0);
        assert!(!r["errors"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Helper: ZIP *stored* con varias entradas, armado a mano.
    fn zip_de(archivos: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z: Vec<u8> = Vec::new();
        let mut centrales: Vec<(u32, &str, usize)> = Vec::new();
        for (nombre, contenido) in archivos {
            let offset = z.len() as u32;
            let n = nombre.as_bytes();
            z.extend(0x0403_4b50u32.to_le_bytes());
            z.extend([20u8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            z.extend(0u32.to_le_bytes());
            z.extend((contenido.len() as u32).to_le_bytes());
            z.extend((contenido.len() as u32).to_le_bytes());
            z.extend((n.len() as u16).to_le_bytes());
            z.extend(0u16.to_le_bytes());
            z.extend(n);
            z.extend(*contenido);
            centrales.push((offset, nombre, contenido.len()));
        }
        let inicio_central = z.len() as u32;
        for (offset, nombre, largo) in &centrales {
            let n = nombre.as_bytes();
            z.extend(0x0201_4b50u32.to_le_bytes());
            z.extend([20u8, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            z.extend(0u32.to_le_bytes());
            z.extend((*largo as u32).to_le_bytes());
            z.extend((*largo as u32).to_le_bytes());
            z.extend((n.len() as u16).to_le_bytes());
            z.extend([0u8; 2]);
            z.extend([0u8; 2]);
            z.extend([0u8; 2]);
            z.extend([0u8; 2]);
            z.extend(0u32.to_le_bytes());
            z.extend(offset.to_le_bytes());
            z.extend(n);
        }
        let largo_central = z.len() as u32 - inicio_central;
        z.extend(0x0605_4b50u32.to_le_bytes());
        z.extend([0u8; 4]);
        z.extend((centrales.len() as u16).to_le_bytes());
        z.extend((centrales.len() as u16).to_le_bytes());
        z.extend(largo_central.to_le_bytes());
        z.extend(inicio_central.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z
    }
}
