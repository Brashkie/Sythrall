//! **Historial de Project Health — Fase 35, portado desde `routers/history.py`.**
//!
//! Guarda un snapshot de los cuatro scores por proyecto y por fecha, para
//! poder comparar dos corridas.
//!
//! **Y cambia el almacenamiento, a propósito.** La versión Python usaba
//! SQLite (`sqlite3` de la stdlib). Portarlo tal cual habría significado
//! meterle al motor una dependencia de SQLite para guardar unas decenas de
//! registros de cuatro números cada uno — exactamente el bloatware que este
//! proyecto evita. Acá se usa **el mismo CBOR append-only de `logstore.rs`**,
//! que ya estaba y no agrega nada nuevo.
//!
//! **Se podía porque no había nada que migrar**: la base tenía **0 filas**
//! cuando se hizo el cambio. Con snapshots reales adentro, la decisión
//! correcta habría sido otra — convertirlos primero.
//!
//! **El costo de no tener índices, dicho:** `overview` y `compare` leen el
//! archivo entero y filtran en memoria. Con decenas de snapshots eso es
//! irrelevante; si algún día son decenas de miles, el que lo note tiene acá
//! el motivo escrito en vez de un misterio.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Las cuatro dimensiones del score. Cambiarlas rompe la comparación con
/// snapshots viejos, y por eso `scoring_version` viaja en cada uno.
pub const CATEGORIAS: &[&str] = &["security", "quality", "complexity", "architecture"];

/// Las claves de `summary` que tienen un delta aritmético con sentido.
/// `big_o_distribution` queda afuera a propósito: es un mapa, no un número.
pub const CLAVES_RESUMEN: &[&str] = &[
    "total_files",
    "total_functions",
    "total_classes",
    "total_imports",
    "unused_imports",
    "wasm_candidates",
    "security_findings",
    "structural_smells",
    "naming_smells",
    "modernization_candidates",
    "architecture_smells",
    "total_loc",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub id: u64,
    pub project_id: String,
    pub created_at: String,
    pub health: Value,
    pub summary: Value,
    /// Qué sabía analizar el motor cuando se midió. `None` en un snapshot
    /// anterior a que esto se guardara — y esa diferencia importa, ver
    /// `diff_de_capacidades`.
    pub capabilities: Option<Vec<String>>,
}

fn directorio() -> PathBuf {
    PathBuf::from(std::env::var("SYTHRALL_HISTORY_DIR").unwrap_or_else(|_| "data".to_string()))
}

fn archivo() -> PathBuf {
    directorio().join("history.cbor")
}

/// El contador de ids, en un archivo hermano de seis bytes.
///
/// **Existe para no reciclar un id.** `max(ids) + 1` parece equivalente y no
/// lo es: borrás el snapshot más alto, guardás otro, y el nuevo se queda con
/// el id del borrado. Cualquier comparación guardada o link a
/// `/history/compare/3/7` empieza a resolver a un snapshot distinto **sin
/// error**, que es la peor forma de equivocarse. SQLite lo evitaba con
/// `AUTOINCREMENT`, y portar esto perdiendo esa garantía habría sido una
/// regresión silenciosa.
fn archivo_de_ids() -> PathBuf {
    directorio().join("history.next")
}

/// Reserva el próximo id y lo persiste.
///
/// `max(contador, mayor_vivo + 1)` cubre los dos lados: el contador impide
/// reciclar un id borrado, y el `mayor_vivo + 1` arranca bien un historial
/// que ya existía antes de que este archivo existiera, o se recupera si el
/// contador se perdió.
fn reservar_id(snaps: &[Snapshot]) -> u64 {
    reservar_id_en(&archivo_de_ids(), snaps)
}

fn reservar_id_en(path: &Path, snaps: &[Snapshot]) -> u64 {
    let guardado = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok())
        .unwrap_or(1);
    let mayor_vivo = snaps.iter().map(|s| s.id).max().unwrap_or(0);
    let id = guardado.max(mayor_vivo + 1);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Si esto falla, el `mayor_vivo + 1` sigue dando un id usable: se pierde
    // la garantía de no reciclar, no la capacidad de guardar.
    let _ = std::fs::write(path, (id + 1).to_string());
    id
}

pub fn leer_todos() -> Vec<Snapshot> {
    leer_de(&archivo())
}

fn leer_de(path: &Path) -> Vec<Snapshot> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut cursor = io::Cursor::new(bytes);
    let mut out = Vec::new();
    // Mismo formato que `logstore`: registros CBOR uno detrás del otro. Un
    // archivo cortado a la mitad devuelve lo que alcanzó a escribirse entero
    // en vez de fallar — un corte de luz no tiene por qué perder el historial.
    while let Ok(s) = ciborium::from_reader::<Snapshot, _>(&mut cursor) {
        out.push(s);
    }
    out
}

fn escribir_todos(path: &Path, snaps: &[Snapshot]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut buf = Vec::new();
    for s in snaps {
        ciborium::into_writer(s, &mut buf).map_err(|e| io::Error::other(e.to_string()))?;
    }
    std::fs::write(path, buf)
}

pub fn guardar(
    project_id: &str,
    health: Value,
    summary: Value,
    capabilities: Option<Vec<String>>,
) -> io::Result<Snapshot> {
    let path = archivo();
    let todos = leer_de(&path);
    let id = reservar_id(&todos);
    let s = Snapshot {
        id,
        project_id: project_id.to_string(),
        created_at: crate::servicio::logstore::now_string(),
        health,
        summary,
        capabilities,
    };
    // Append puro: no se reescribe lo ya guardado.
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut buf = Vec::new();
    ciborium::into_writer(&s, &mut buf).map_err(|e| io::Error::other(e.to_string()))?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(&buf)?;
    Ok(s)
}

pub fn de_proyecto(project_id: &str) -> Vec<Snapshot> {
    let mut v: Vec<Snapshot> =
        leer_todos().into_iter().filter(|s| s.project_id == project_id).collect();
    // Más nuevo primero, igual que devolvía el `ORDER BY created_at DESC`.
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    v
}

pub fn por_id(id: u64) -> Option<Snapshot> {
    leer_todos().into_iter().find(|s| s.id == id)
}

pub fn borrar(id: u64) -> io::Result<bool> {
    let path = archivo();
    let todos = leer_de(&path);
    let quedan: Vec<Snapshot> = todos.iter().filter(|s| s.id != id).cloned().collect();
    if quedan.len() == todos.len() {
        return Ok(false);
    }
    // **Reescribe el archivo entero.** Es append-only para guardar, pero un
    // borrado no se puede hacer agregando; con este volumen reescribir es más
    // simple y honesto que llevar tumbas que después hay que compactar.
    escribir_todos(&path, &quedan)?;
    Ok(true)
}

fn score(health: &Value, categoria: &str) -> i64 {
    health.get(categoria).and_then(|c| c.get("score")).and_then(Value::as_i64).unwrap_or(0)
}

fn numero(summary: &Value, clave: &str) -> i64 {
    summary.get(clave).and_then(Value::as_i64).unwrap_or(0)
}

/// **Si las dos mediciones usaron fórmulas distintas, decirlo.**
///
/// Hermano exacto de `diff_de_capacidades`, y por el mismo motivo: allá el
/// analizador aprendió a ver algo nuevo, acá cambió la forma de convertir
/// hallazgos en score. En los dos casos el delta **no es un cambio del
/// código**, y sin avisarlo se lee como una mejora o una regresión real.
pub fn diff_de_scoring(antes: &Value, despues: &Value) -> Value {
    let v = |h: &Value| h.get("scoring_version").and_then(Value::as_i64).unwrap_or(1);
    let (a, b) = (v(antes), v(despues));
    if a == b {
        return json!({ "from": a, "to": b, "comparable": true, "note": "" });
    }
    json!({
        "from": a, "to": b, "comparable": false,
        "note": format!(
            "Las dos mediciones usan formulas de score distintas (v{a} y v{b}): \
             el delta de abajo mezcla un cambio de tu codigo con un cambio del calculo. \
             Para comparar de verdad, volve a analizar y guarda un snapshot nuevo."
        ),
    })
}

/// **Qué cambió en el ANALIZADOR entre las dos mediciones.**
///
/// Es la diferencia entre *"tu código empeoró"* y *"Sythrall aprendió a ver
/// algo que antes no veía"*. Pasó cuatro veces en este repo: un lenguaje que
/// devolvía `[]` fijo empezó a emitir hallazgos reales y el score de
/// cualquier proyecto de ese lenguaje bajó sin tocar una línea.
///
/// Un `None` significa que ese snapshot es anterior a que esto se guardara —
/// **no** que no tuviera capacidades. Se dice explícitamente en vez de
/// asumir que no cambió nada, que es la suposición que hace daño.
pub fn diff_de_capacidades(antes: Option<&Vec<String>>, despues: Option<&Vec<String>>) -> Value {
    let (Some(a), Some(b)) = (antes, despues) else {
        return json!({
            "known": false, "comparable": false, "added": [], "removed": [],
            "note": "Al menos uno de los dos snapshots es anterior a que se guardara qué sabía analizar el motor. \
                     Una diferencia de score entre ellos puede venir de un cambio en el código o de un análisis \
                     nuevo que antes no existía, y desde acá no hay forma de distinguirlos.",
        });
    };
    let sa: std::collections::BTreeSet<&String> = a.iter().collect();
    let sb: std::collections::BTreeSet<&String> = b.iter().collect();
    let agregadas: Vec<&&String> = sb.difference(&sa).collect();
    let quitadas: Vec<&&String> = sa.difference(&sb).collect();
    if agregadas.is_empty() && quitadas.is_empty() {
        return json!({
            "known": true, "comparable": true, "added": [], "removed": [],
            "note": "El motor analizaba lo mismo en las dos mediciones: la diferencia de score es del código.",
        });
    }
    json!({
        "known": true, "comparable": false,
        "added": agregadas, "removed": quitadas,
        "note": format!(
            "El motor cambió entre las dos mediciones ({} análisis nuevos, {} retirados). \
             Parte de la diferencia de score puede venir de eso y no del código — un análisis que antes no \
             existía encuentra problemas que ya estaban ahí.",
            agregadas.len(), quitadas.len()
        ),
    })
}

/// `from`/`to` no tienen por qué estar en orden cronológico: el delta siempre
/// es `to - from`, tal cual se pidió.
pub fn comparar(desde: u64, hasta: u64) -> Option<Value> {
    let todos = leer_todos();
    let a = todos.iter().find(|s| s.id == desde)?;
    let b = todos.iter().find(|s| s.id == hasta)?;
    Some(comparar_pares(a, b))
}

/// La comparación propiamente dicha, sin tocar disco — `comparar` es solo la
/// capa que busca los dos snapshots. Separado para poder testear el diff con
/// snapshots armados a mano, sin variables de entorno que los tests en
/// paralelo se pisarían entre sí.
pub fn comparar_pares(a: &Snapshot, b: &Snapshot) -> Value {
    let mut health_diff = Map::new();
    for c in CATEGORIAS {
        health_diff.insert(
            (*c).to_string(),
            json!({ "from": score(&a.health, c), "to": score(&b.health, c),
                    "delta": score(&b.health, c) - score(&a.health, c) }),
        );
    }
    let mut summary_diff = Map::new();
    for k in CLAVES_RESUMEN {
        summary_diff.insert(
            (*k).to_string(),
            json!({ "from": numero(&a.summary, k), "to": numero(&b.summary, k),
                    "delta": numero(&b.summary, k) - numero(&a.summary, k) }),
        );
    }
    json!({
        "from": a, "to": b,
        "health_diff": health_diff,
        "summary_diff": summary_diff,
        "capabilities_diff": diff_de_capacidades(a.capabilities.as_ref(), b.capabilities.as_ref()),
        "scoring_diff": diff_de_scoring(&a.health, &b.health),
    })
}

/// Redondeo al entero más cercano, con el medio hacia arriba.
///
/// **Difiere a propósito de la versión Python**, y es la única diferencia de
/// número entre las dos: el `round()` de Python es *bancario*
/// (`round(68.5) == 68`, `round(67.5) == 68` — el medio va al par), así que
/// un promedio que caía exactamente en `.5` daba un punto menos. Se eligió
/// el redondeo de siempre porque es el que alguien que ve "68.5 → 69"
/// espera, y porque el bancario en un score mostrado al usuario no compra
/// nada: no se están sumando miles de valores donde el sesgo acumulado
/// importe. Queda escrito para que la diferencia de un punto contra un
/// snapshot viejo tenga explicación en vez de ser un misterio.
fn redondear(x: f64) -> i64 {
    x.round() as i64
}

/// Panorama entre todos los proyectos con historial. Nada acá recalcula un
/// análisis: todo sale de snapshots ya guardados.
pub fn panorama(nombres: &std::collections::BTreeMap<String, Option<String>>) -> Value {
    panorama_de(leer_todos(), nombres)
}

/// El panorama sobre una lista ya leída — ver `comparar_pares` para por qué
/// está separado de la lectura.
pub fn panorama_de(
    todos: Vec<Snapshot>,
    nombres: &std::collections::BTreeMap<String, Option<String>>,
) -> Value {
    let total_snapshots = todos.len();
    let mut por_proyecto: std::collections::BTreeMap<String, Vec<Snapshot>> =
        std::collections::BTreeMap::new();
    for s in todos {
        por_proyecto.entry(s.project_id.clone()).or_default().push(s);
    }

    let mut proyectos: Vec<Value> = Vec::new();
    for (pid, mut snaps) in por_proyecto {
        snaps.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        let ultimo = &snaps[0];
        let anterior = snaps.get(1);
        let total: i64 = CATEGORIAS.iter().map(|c| score(&ultimo.health, c)).sum();
        let overall = redondear(total as f64 / CATEGORIAS.len() as f64);
        let delta = anterior.map(|p| {
            let mut m = Map::new();
            for c in CATEGORIAS {
                m.insert((*c).to_string(), json!(score(&ultimo.health, c) - score(&p.health, c)));
            }
            Value::Object(m)
        });
        proyectos.push(json!({
            "project_id": pid,
            "project_name": nombres.get(&pid).cloned().flatten(),
            "exists": nombres.contains_key(&pid),
            "snapshots": snaps.len(),
            "created_at": ultimo.created_at,
            "health": ultimo.health,
            "summary": ultimo.summary,
            "overall": overall,
            "delta": delta,
        }));
    }
    // Peor primero: el panorama existe para mirar lo que está mal.
    proyectos.sort_by_key(|p| p.get("overall").and_then(Value::as_i64).unwrap_or(0));

    let n = proyectos.len();
    let mut avg = Map::new();
    for c in CATEGORIAS {
        let suma: i64 = proyectos
            .iter()
            .map(|p| p.get("health").map_or(0, |h| score(h, c)))
            .sum();
        avg.insert(
            (*c).to_string(),
            json!(if n == 0 { 0 } else { redondear(suma as f64 / n as f64) }),
        );
    }
    let suma_avg: i64 = CATEGORIAS.iter().map(|c| avg[*c].as_i64().unwrap_or(0)).sum();
    avg.insert(
        "overall".to_string(),
        json!(redondear(suma_avg as f64 / CATEGORIAS.len() as f64)),
    );
    let mut totales = Map::new();
    for k in CLAVES_RESUMEN {
        let suma: i64 =
            proyectos.iter().map(|p| p.get("summary").map_or(0, |s| numero(s, k))).sum();
        totales.insert((*k).to_string(), json!(suma));
    }

    json!({
        "projects": proyectos,
        "aggregate": {
            "projects": n,
            "snapshots": total_snapshots,
            "avg": avg,
            "totals": totales,
        },
        "note": "Agregado sobre el último snapshot guardado de cada proyecto, no sobre un análisis en vivo — \
                 un proyecto que cambió desde su último snapshot se muestra como estaba entonces.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salud(s: i64, version: i64) -> Value {
        json!({
            "security": {"score": s}, "quality": {"score": s},
            "complexity": {"score": s}, "architecture": {"score": s},
            "scoring_version": version,
        })
    }

    #[test]
    fn misma_version_se_declara_comparable() {
        let d = diff_de_scoring(&salud(80, 2), &salud(90, 2));
        assert_eq!(d["comparable"], true);
        assert_eq!(d["note"], "");
    }

    /// **El delta entre fórmulas distintas no es del código, y hay que
    /// decirlo.** Sin este aviso, cambiar la fórmula se lee como que el
    /// proyecto mejoró o empeoró solo.
    #[test]
    fn versiones_distintas_avisan_que_el_delta_no_es_del_codigo() {
        let d = diff_de_scoring(&salud(80, 1), &salud(90, 2));
        assert_eq!(d["comparable"], false);
        assert_eq!(d["from"], 1);
        assert_eq!(d["to"], 2);
        assert!(d["note"].as_str().unwrap().to_lowercase().contains("formulas"));
    }

    /// Un snapshot sin `scoring_version` es v1 por definición — justamente el
    /// que no es comparable con el actual.
    #[test]
    fn un_snapshot_viejo_sin_version_cuenta_como_v1() {
        let viejo = json!({ "security": {"score": 50} });
        let d = diff_de_scoring(&viejo, &salud(50, 2));
        assert_eq!(d["from"], 1);
        assert_eq!(d["comparable"], false);
    }

    /// **`None` no es "no cambió nada".** Es "no se sabe", y asumir lo
    /// primero es la suposición que hace daño.
    #[test]
    fn capacidades_desconocidas_no_se_dan_por_iguales() {
        let d = diff_de_capacidades(None, Some(&vec!["a".to_string()]));
        assert_eq!(d["known"], false);
        assert_eq!(d["comparable"], false);
        assert!(d["note"].as_str().unwrap().contains("anterior"));
    }

    #[test]
    fn las_mismas_capacidades_declaran_que_la_diferencia_es_del_codigo() {
        let a = vec!["x".to_string(), "y".to_string()];
        let b = vec!["y".to_string(), "x".to_string()];
        let d = diff_de_capacidades(Some(&a), Some(&b));
        assert_eq!(d["comparable"], true);
        assert!(d["note"].as_str().unwrap().contains("del código"));
    }

    #[test]
    fn un_analisis_nuevo_se_reporta_como_cambio_del_motor() {
        let a = vec!["x".to_string()];
        let b = vec!["x".to_string(), "nuevo".to_string()];
        let d = diff_de_capacidades(Some(&a), Some(&b));
        assert_eq!(d["comparable"], false);
        assert_eq!(d["added"].as_array().unwrap().len(), 1);
        assert_eq!(d["removed"].as_array().unwrap().len(), 0);
    }

    /// El formato de disco es el mismo CBOR append-only de `logstore`.
    #[test]
    fn roundtrip_en_disco_conserva_todo() {
        let dir = std::env::temp_dir().join(format!("hist-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("h.cbor");
        let _ = std::fs::remove_file(&path);
        let snaps = vec![
            Snapshot {
                id: 1,
                project_id: "p".into(),
                created_at: "2026-01-01 00:00:00".into(),
                health: salud(70, 2),
                summary: json!({"total_files": 3}),
                capabilities: Some(vec!["a".into()]),
            },
            Snapshot {
                id: 2,
                project_id: "p".into(),
                created_at: "2026-01-02 00:00:00".into(),
                health: salud(80, 2),
                summary: json!({"total_files": 4}),
                capabilities: None,
            },
        ];
        escribir_todos(&path, &snaps).expect("no se pudo escribir");
        let leidos = leer_de(&path);
        assert_eq!(leidos.len(), 2);
        assert_eq!(leidos[1].id, 2);
        assert_eq!(leidos[0].capabilities.as_ref().unwrap()[0], "a");
        assert!(leidos[1].capabilities.is_none(), "el None tiene que sobrevivir al roundtrip");
        let _ = std::fs::remove_file(&path);
    }

    fn snap(id: u64, pid: &str, fecha: &str, s: i64) -> Snapshot {
        Snapshot {
            id,
            project_id: pid.into(),
            created_at: fecha.into(),
            health: salud(s, 2),
            summary: json!({"total_files": 1, "naming_smells": 2}),
            capabilities: None,
        }
    }

    fn nombres(pares: &[(&str, Option<&str>)]) -> std::collections::BTreeMap<String, Option<String>> {
        pares.iter().map(|(k, v)| ((*k).to_string(), v.map(str::to_string))).collect()
    }

    /// **El panorama existe para mirar lo que está mal**, así que el peor va
    /// primero — no el alfabético ni el cronológico.
    #[test]
    fn el_panorama_pone_el_peor_proyecto_primero() {
        let snaps = vec![
            snap(1, "sano", "2026-01-01 00:00:00", 95),
            snap(2, "roto", "2026-01-01 00:00:00", 20),
        ];
        let p = panorama_de(snaps, &nombres(&[("sano", Some("Sano")), ("roto", Some("Roto"))]));
        let ids: Vec<&str> =
            p["projects"].as_array().unwrap().iter().map(|x| x["project_id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["roto", "sano"]);
        assert_eq!(p["aggregate"]["projects"], 2);
        assert_eq!(p["aggregate"]["snapshots"], 2);
    }

    /// **`delta` es `None` con un solo snapshot, y eso no es lo mismo que
    /// cero.** Cero diría "no cambió nada"; `None` dice "no hay con qué
    /// comparar todavía".
    #[test]
    fn con_un_solo_snapshot_el_delta_es_nulo_y_no_cero() {
        let p = panorama_de(
            vec![snap(1, "p", "2026-01-01 00:00:00", 70)],
            &nombres(&[("p", Some("P"))]),
        );
        assert!(p["projects"][0]["delta"].is_null());
        assert_eq!(p["projects"][0]["snapshots"], 1);
    }

    #[test]
    fn el_delta_compara_el_ultimo_contra_el_anterior() {
        let snaps = vec![
            snap(1, "p", "2026-01-01 00:00:00", 60),
            snap(2, "p", "2026-02-01 00:00:00", 75),
        ];
        let p = panorama_de(snaps, &nombres(&[("p", None)]));
        assert_eq!(p["projects"][0]["overall"], 75, "el ultimo es el de fecha mas nueva");
        assert_eq!(p["projects"][0]["delta"]["security"], 15);
    }

    /// **Un proyecto borrado del disco no se esconde.** Su score histórico
    /// sigue siendo un dato real; lo que cambia es `exists`.
    #[test]
    fn un_proyecto_que_ya_no_esta_en_disco_se_marca_pero_no_se_oculta() {
        let p = panorama_de(vec![snap(1, "fantasma", "2026-01-01 00:00:00", 50)], &nombres(&[]));
        assert_eq!(p["projects"].as_array().unwrap().len(), 1);
        assert_eq!(p["projects"][0]["exists"], false);
        assert!(p["projects"][0]["project_name"].is_null());
    }

    /// **Existir sin nombre guardado no es lo mismo que no existir.** La
    /// metadata de los proyectos subidos antes de que hubiera un campo de
    /// nombre no lo trae, y el panorama tiene que distinguir los dos casos.
    #[test]
    fn existir_sin_nombre_se_distingue_de_no_existir() {
        let snaps =
            vec![snap(1, "sin-nombre", "2026-01-01 00:00:00", 50), snap(2, "ido", "2026-01-01 00:00:00", 50)];
        let p = panorama_de(snaps, &nombres(&[("sin-nombre", None)]));
        let por_id: std::collections::BTreeMap<&str, &Value> = p["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x["project_id"].as_str().unwrap(), x))
            .collect();
        assert_eq!(por_id["sin-nombre"]["exists"], true);
        assert!(por_id["sin-nombre"]["project_name"].is_null());
        assert_eq!(por_id["ido"]["exists"], false);
    }

    /// **El promedio entre proyectos no se pondera por tamaño**: un proyecto
    /// chico con seguridad rota cuenta igual que uno grande sano. Ponderar
    /// por LOC esconderia justo el caso que este panorama existe para mostrar.
    #[test]
    fn el_promedio_no_se_pondera_por_tamano() {
        let mut chico = snap(1, "chico", "2026-01-01 00:00:00", 0);
        chico.summary = json!({"total_loc": 10});
        let mut grande = snap(2, "grande", "2026-01-01 00:00:00", 100);
        grande.summary = json!({"total_loc": 100_000});
        let p = panorama_de(vec![chico, grande], &nombres(&[]));
        assert_eq!(p["aggregate"]["avg"]["security"], 50, "promedio simple, no ponderado por LOC");
        assert_eq!(p["aggregate"]["totals"]["total_loc"], 100_010);
    }

    /// **El medio va para arriba, y Python daba lo contrario.** `round()` de
    /// Python es bancario: `round(68.5) == 68`. Este test fija la diferencia
    /// para que no se "corrija" por accidente hacia el comportamiento viejo.
    #[test]
    fn el_medio_redondea_para_arriba_a_diferencia_de_python() {
        assert_eq!(redondear(68.5), 69);
        assert_eq!(redondear(67.5), 68);
        assert_eq!(redondear(0.5), 1);
        let snaps = vec![
            snap(1, "a", "2026-01-01 00:00:00", 37),
            snap(2, "b", "2026-01-01 00:00:00", 100),
        ];
        let p = panorama_de(snaps, &nombres(&[]));
        // (37 + 100) / 2 = 68.5 → 69 aca, 68 en Python.
        assert_eq!(p["aggregate"]["avg"]["quality"], 69);
    }

    /// **Un id borrado no se reutiliza.** `max(ids) + 1` lo habría
    /// reutilizado, y una comparación guardada a ese id empezaría a resolver
    /// a otro snapshot **sin error** — la peor forma de equivocarse.
    #[test]
    fn un_id_borrado_no_se_recicla() {
        let dir = std::env::temp_dir().join(format!("hist-id-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("next");
        let _ = std::fs::remove_file(&path);

        let mut vivos = vec![
            snap(1, "p", "2026-01-01 00:00:00", 50),
            snap(2, "p", "2026-01-02 00:00:00", 50),
            snap(3, "p", "2026-01-03 00:00:00", 50),
        ];
        // Arranca un historial que ya existia sin contador: se recupera del
        // mayor vivo.
        assert_eq!(reservar_id_en(&path, &vivos), 4);
        vivos.push(snap(4, "p", "2026-01-04 00:00:00", 50));

        // Se borra el mas alto. El proximo id tiene que seguir subiendo.
        vivos.retain(|s| s.id != 4);
        assert_eq!(reservar_id_en(&path, &vivos), 5, "el 4 estaba usado: no se vuelve a dar");

        // Y se borra todo: tampoco se vuelve al 1.
        vivos.clear();
        assert_eq!(reservar_id_en(&path, &vivos), 6, "un historial vaciado no reinicia los ids");
        let _ = std::fs::remove_file(&path);
    }

    /// `from`/`to` no tienen por que venir en orden cronologico: el delta es
    /// `to - from` tal cual se pidio, en la direccion que se pidio.
    #[test]
    fn el_delta_respeta_la_direccion_pedida() {
        let a = snap(1, "p", "2026-01-01 00:00:00", 60);
        let b = snap(2, "p", "2026-02-01 00:00:00", 80);
        let adelante = comparar_pares(&a, &b);
        let atras = comparar_pares(&b, &a);
        assert_eq!(adelante["health_diff"]["security"]["delta"], 20);
        assert_eq!(atras["health_diff"]["security"]["delta"], -20);
        assert_eq!(atras["from"]["id"], 2);
    }

    /// **`big_o_distribution` no entra en el diff**: es un mapa, no tiene un
    /// delta aritmetico con sentido. Y una clave ausente en un snapshot
    /// viejo cuenta como 0 en vez de reventar.
    #[test]
    fn el_diff_de_resumen_ignora_el_mapa_y_tolera_claves_ausentes() {
        let mut a = snap(1, "p", "2026-01-01 00:00:00", 50);
        a.summary = json!({"big_o_distribution": {"O(n)": 3}});
        let mut b = snap(2, "p", "2026-02-01 00:00:00", 50);
        b.summary = json!({"total_files": 7, "big_o_distribution": {"O(n²)": 1}});
        let d = comparar_pares(&a, &b);
        assert!(d["summary_diff"].get("big_o_distribution").is_none());
        assert_eq!(d["summary_diff"]["total_files"]["from"], 0, "clave ausente = 0");
        assert_eq!(d["summary_diff"]["total_files"]["delta"], 7);
        assert_eq!(
            d["summary_diff"].as_object().unwrap().len(),
            CLAVES_RESUMEN.len(),
            "el diff trae exactamente las claves numericas declaradas"
        );
    }

    /// Un archivo cortado a la mitad devuelve lo que se escribió entero en
    /// vez de perder el historial completo.
    #[test]
    fn un_archivo_truncado_devuelve_lo_que_alcanzo_a_escribirse() {
        let dir = std::env::temp_dir().join(format!("hist-t-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.cbor");
        let snaps = vec![Snapshot {
            id: 1,
            project_id: "p".into(),
            created_at: "2026-01-01 00:00:00".into(),
            health: salud(70, 2),
            summary: json!({}),
            capabilities: None,
        }];
        escribir_todos(&path, &snaps).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(&[0xa5, 0x01, 0x02]); // un registro a medias
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(leer_de(&path).len(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
