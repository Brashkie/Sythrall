# Roadmap

Detalle completo fase por fase — alcance, razonamiento, qué se shippeó, qué sigue
abierto. Ver [README.es.md](README.es.md) para la descripción del producto, o
[CHANGELOG.md](CHANGELOG.md) para el historial de versiones (un eje distinto — una
fase agrupa un bloque coherente de trabajo a lo largo de la vida del proyecto, un
release/tag lleva su propio número semántico). Misma convención que usa el roadmap
propio de [`ansimax`](https://github.com/Brashkie/ansimax).

[English](./ROADMAP.md) · [Español](./ROADMAP.es.md)

**Estado**: ✅ Completa · 🟡 Parcial · 🔴 Planeada

## Filosofía de lenguajes

Sythrall no está "construido con seis lenguajes" — cada lenguaje ocupa la capa donde
realmente aporta valor, y las Fases 13+ de abajo están organizadas según esa división
en vez de una lista plana de "agregar más lenguajes":

| Capa | Lenguaje | Rol | Evidencia hasta ahora |
|---|---|---|---|
| Interacción | **TypeScript** | UI, integración con Monaco, editor intelligence, diagramas — la única capa que el usuario toca directamente | Vite + Monaco + Mermaid + Chart.js + xterm, todo shippeado (Fases 1–9) |
| Intelligence & Science | **Python** | IA/ML, orquestación, cargas científicas — **no** el motor de análisis estático a largo plazo: ese rol se está retirando hacia Rust (Native Analysis Core de la Fase 18), una porción medida a la vez | 23 librerías detectadas, ML/DL Inspector (Fase 2). `static_parser.py` en sí es la pieza legacy que la Fase 18 está migrando fuera de Python, no un fixture permanente |
| Native Analysis | **Rust** | El motor de análisis estático, destino comprometido — parsing, AST, resolución de símbolos, complejidad, seguridad, calidad, grafos. Cada porción se sigue perfilando/benchmarkeando antes del swap (eso es el *cómo*, no el *si*) | `terminal-server` (manejo de PTY), `complexity-engine` (9–21× más rápido que `radon` — Fase 11), análisis Python rico portado (Fase 18) |
| Scientific/HPC | **Fortran** | *Objetivo* de análisis, no lenguaje de implementación — Sythrall ya tiene rendimiento numérico nivel Fortran gratis vía los backends LAPACK/BLAS compilados de numpy/scipy | Planeado (Fase 20) |
| Nivel máquina | **Assembly** | Objetivo de análisis para desglose de instrucciones/registros/control-flow — envuelve Capstone/LIEF en vez de escribir un disassembler a mano | Planeado (Fase 19) |
| Native tooling | **Zig** | Build, cross-compilación, distribución standalone — una preocupación distinta al rol de motor de análisis de Rust, no compite con él | Planeado (Fase 25) |

La regla para mover cualquier cosa a Rust (o cualquier lenguaje nativo) solía ser
un either/or de verdad: perfilar primero, benchmarkear el reemplazo, y quedarse con
él solo si los números lo justifican — la investigación de proyectos gigantes de la
Fase 10 encontró que el costo real O(n²) eran tres bugs de Python comunes, no el
parser, y se arregló sin lenguaje nuevo; el `complexity-engine` de la Fase 11
encontró una ganancia real, medida, de 9–21× y adoptó Rust. Los dos resultados
salieron del mismo proceso; ninguno se asumió de entrada.

Para el motor de análisis estático puntualmente, esa pregunta ya está resuelta: se
muda a Rust, completo, no solo donde un benchmark lo favorezca — ver el Native
Analysis Core de la Fase 18 más abajo. Lo que sobrevive de la regla vieja es el
*método*, no la *decisión*: cada porción se sigue portando, testeando por paridad
contra el Python que reemplaza, y benchmarkeando antes de que los call sites
cambien — la migración nunca cambia correctitud por velocidad de llegar a la meta.
El framing either/or todavía aplica a cualquier *otra* adopción futura de lenguaje
nativo fuera de esta migración puntual — esto es una excepción explícita y única,
no una reversión de la regla general.

## Hacia dónde va esto: Computer Science Intelligence, no un linter

La descripción honesta de Sythrall hoy es "lee código, calcula Big-O." Las Fases
13–23 de abajo son el plan para hacer crecer eso hacia algo más específico:
*Sythrall analiza software desde las estructuras matemáticas/algorítmicas de abajo,
hasta el compilador, el código máquina, y el hardware sobre el que corre.* No
convirtiéndose en un solver matemático ni en un compilador — conectando la teoría de
CS que ya explica *por qué* funcionan las heurísticas que Sythrall ya tiene (Big-O,
los clasificadores de la jerarquía de Chomsky de la Fase 8/12, el framing de Cálculo
Lambda sobre recursión tail-call) con el resto de la teoría de la que salen esas
ideas, y construyendo detectores para eso con el mismo método heurístico y
benchmark-primero con el que se construyó todo lo demás de este roadmap:

```
                    Computer Science Engine
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                      │
   Algoritmos             Matemática              Lenguajes
        │                     │                      │
    Big-O/Θ/Ω            Matemática Discreta    Lenguajes Formales
    Estructuras de Datos  Lógica                Gramáticas
    Grafos                Recurrencias          Teoría de Parsing
        │                     │                      │
        └─────────────────────┼──────────────────────┘
                              │
                       Teoría de Compiladores
                              │
                    Lexer → AST → IR → Codegen
                              │
                        Código Máquina
                              │
                             CPU
```

Cada capa de abajo sigue gateada por las mismas reglas ya establecidas en este
roadmap: heurística y explícita sobre serlo (mismo estilo que los detectores de
WASM-hints/clasificadores CS Engine, no afirmando prueba semántica), y cualquier
adopción de un lenguaje nativo sigue necesitando su propio benchmark antes/después
(la regla de la Fase 18, ya probada dos veces).

## Fases

### ✅ Fase 1 — Base
- [x] Backend Flask + pylint, flake8, radon
- [x] Frontend TypeScript + Vite sin frameworks
- [x] Monaco Editor + Chart.js + Docker

### ✅ Fase 2 — Inspector ML/DL
- [x] 23 librerías detectadas, 23 patrones de pipeline, 25 modelos
- [x] 20+ reglas de issues (data leakage, reproducibilidad, frameworks)
- [x] Score 0–100 + diagrama Mermaid del pipeline

### ✅ Fase 3 — Zoom/Pan + Responsive
- [x] Zoom y pan en diagramas, layout responsive completo, bottom navigation móvil

### ✅ Fase 4 — FastAPI + Upload de proyectos
- [x] Migración Flask → FastAPI, upload de archivos/carpetas/ZIPs, 83 tests

### ✅ Fase 5 — Análisis Estático + Editor Intelligence
- [x] Parser AST multi-lenguaje (Python · TypeScript · C/C++) sin IA
- [x] Estimación Big-O por función, complejidad ciclomática, hints Cython/WASM
- [x] Linting en tiempo real (fast ~1ms, heavy ~80ms), diagnósticos inline
- [x] Hover: firma + Big-O + CC + docstring
- [x] Go to Definition · Find References · Autocompletado semántico · Rename Symbol
- [x] 162 tests automatizados

### ✅ Fase 6 — Code Graph + Project Explorer
- [x] Import Graph · Call Graph · Dependencias Circulares
- [x] Sub-etapa A: grafos desde sidebar; Sub-etapa B: desde proyectos ZIP completos
- [x] Resolución cross-folder, detección circular con NetworkX
- [x] Force Graph interactivo *y* dir-tree de Complexity Heatmap (`renderForceGraph`/`renderDirTree` en `panels/graph.ts`, motor de física propio, sin D3) *(los dos implementados y testeados — un audit posterior encontró que todo el módulo de grafos no tenía ni un caller en la app; el Tree View con Mermaid quedó conectado en la Fase 10 más abajo, pero `generateWholeProjectDiagram` en `app.ts` todavía pasa `onForce`/`onDirTree` como no-ops (documentado en su propio docstring) — ninguno de los dos tiene control de UI todavía)*
- [x] Project Explorer: árbol + tabs múltiples + búsqueda global + outline
- [x] 316 tests automatizados

### ✅ Fase 7 — Panel de Problemas + Métricas en Vivo *(código shippeado, wiring incompleto encontrado después — ver Fases 10 y 12)*
- [x] **Panel de problemas** (estilo VSCode): errores · warnings · Big-O · complejidad · hallazgos de seguridad — *originalmente compartía contenedor DOM con la vista de análisis de archivo y la hubiera pisado; resuelto dándole su propio sub-tab del panel derecho (`#rpp-problems`) en vez de fusionar las dos vistas — ver Fase 12 más abajo*
- [x] **Barra de métricas en vivo** en el editor: LOC · funciones · imports · complexity score · Big-O peor caso · parse time (ms) *(conectado en la Fase 10 más abajo — el módulo existía desde esta fase pero `editor.ts` nunca lo llamaba)*
- [x] Auto-recovery si el parser falla (safe mode + fallback regex)
- [x] Detección de archivos corruptos
- [x] Restauración de sesión *(conectado en la Fase 10 más abajo, junto con persistir el proyecto activo — restaurar "qué archivo estaba abierto" recién tuvo sentido real cuando hubo un proyecto de verdad del cual recuperar el contenido)*

### ✅ Fase 8 — Computer Science Engine *(extensión directa del motor de análisis existente, sin arquitectura nueva)*

No solo "qué" hace el código — *por qué* se comporta así. Construido enteramente sobre datos que `static_parser.py` y el motor de Big-O ya calculan:

- [x] Complejidad completa por función: Θ (cota ajustada), Ω (mejor caso), O (peor caso) — no solo el O del peor caso *(solo Python por ahora; C/C++/JS/TS todavía muestran solo O)*
- [x] Explicación del "por qué" en cada resultado de Big-O (ej. *"2 loops anidados — el loop interno corre n veces por cada iteración del externo"*)
- [x] Recursión detectada → detección de tail-call + marco de "Cálculo Lambda" *(estimación de profundidad omitida — depende del input en runtime, no se puede calcular de forma estática confiable)*
- [x] Regex detectada → clasificar como Autómata Finito / Chomsky Tipo-3 (Regular) *(solo llamadas directas `re.XXX(...)` — no rastrea un `re.Pattern` guardado en variable)*
- [x] Código con forma de gramática/parser detectado → Gramática Libre de Contexto / Autómata con Pila / Chomsky Tipo-2 *(heurística: nombre + forma de recursión/pila explícita — se exigen las dos señales para no generar demasiados falsos positivos)*
- [x] Recorrido de grafo detectado → etiquetar como DFS/BFS/orden topológico, O(V+E) *(heurística: señales de nombre de variable — `visited`/`seen`/`explored`, `in_degree`, una cola con `.popleft()` — no un análisis de flujo de datos real)*

### ✅ Fase 9 — Terminal Integrada + Explorador de Carpetas + Tema

No estaba originalmente en este roadmap — salió directo de feedback del usuario a mitad de desarrollo, se sumó porque cada pieza era chica y estaba bien acotada por separado:

- [x] **Terminal integrada**, shell interactiva real sobre WebSocket — primer uso de **Rust** en el proyecto (sidecar `terminal-server`: `portable-pty` + `axum`), protegida por token, auto-conexión sin fricción para uso local, selector de panel entre la shell y una vista de Logs en vivo
- [x] **Explorador de carpetas** en el sidebar ("+ Carpeta") — árbol expandible estilo VSCode desde una carpeta real del disco, cross-browser vía `webkitdirectory` (deliberadamente *no* la File System Access API, que es solo Chromium)
- [x] Toggle de **tema claro/oscuro**, persistido, oscuro por defecto
- [x] [`ansimax`](https://github.com/Brashkie/ansimax) (librería propia) para el banner de arranque de `npm run dev`

### ✅ Fase 10 — Rebrand + reestructuración a `apps/` + escalado a proyectos gigantes + shell estilo enterprise

No estaba originalmente en este roadmap — salió de querer que el proyecto aguante código real a gran escala y se vea/comporte como las herramientas de referencia mostradas durante el desarrollo (Aikido, Datadog, DeepSource), no de un plan de versiones:

- [x] Renombrado **CodeWatch PRO → Sythrall** en todo el proyecto (nombre del paquete, servicios Docker, identificadores internos, remote de git)
- [x] Reestructurado a `apps/api` · `apps/web` · `apps/terminal` — un directorio por servicio, los manifiestos de cada herramienta se quedan en la raíz (layout estilo Turborepo/Nx) *(`apps/terminal` se mudó a `services/terminal` después, separando productos de cara al usuario de procesos independientes — ver [`CHANGELOG.md`](CHANGELOG.md))*
- [x] **Benchmark de proyectos gigantes**: se armó un harness reproducible con proyectos sintéticos (hasta 4003 archivos, hasta 1600 funciones por archivo) en vez de asumir que hacía falta una reescritura. Encontró y arregló tres bugs reales O(n²) — dos escondidos dentro de comprehensions de una línea, uno un cálculo muerto que el frontend nunca leía. La generación del Import Graph con 4003 archivos pasó de 3.9s a 0.128s (30×) sin agregar ningún lenguaje nuevo. Detalle en [`CHANGELOG.md`](CHANGELOG.md#460). El parser propio (`static_parser.py`) ya era lineal y no necesitó cambios — el hallazgo anterior sobre PyO3 sigue vigente.
- [x] **Nav vertical reemplaza la tabbar horizontal** — nav de iconos persistente (`apps/web/src/utils/icons.ts`, SVGs inline, `stroke="currentColor"` para seguir el tema activo sin código extra), mismo patrón que usan las herramientas de referencia. `switchTab()` no cambió — los items del nav nuevo mantuvieron la convención `class="tab"`/`data-tab`/`id="t-*"`, así que fue un cambio puramente de HTML/CSS.
- [x] **Un solo proyecto activo, no cuatro entradas separadas.** Antes: "+ Código"/"+ Carpeta"/"+ Log" del sidebar eran efímeros (se perdían al refrescar, nunca tocaban el backend) mientras que Proyectos era el único camino persistente — dos modelos mentales para la misma idea. Ahora "+ Código"/"+ Carpeta" crean o suman al **proyecto activo** (mismos endpoints del backend que ya usaba Proyectos, `project_id` ahora opcional en `/api/upload/{files,folder}` para soportar el append), y Editor · Issues · Diagrama · Static · Métricas leen todos del proyecto que esté activo — elegís un proyecto una vez, trabajás en todos los paneles.
- [x] **Arreglos encontrados por audit**, con el mismo método de "¿esto tiene algún caller de verdad?" que encontró el hueco del Force Graph arriba: se reconectaron la Live Metrics Bar y Session Restore (`panels/problems.ts`, escrito para la Fase 7, nunca llamado desde `editor.ts`); el proyecto activo ahora persiste en `localStorage` así que ambos se restauran solos al recargar; se arregló el badge de la pestaña APIs (nunca se actualizaba); el panel de Métricas ganó un modo de proyecto activo igual que Issues/Diagrama/Static.
- [x] Ubicación del **panel de problemas** — todavía necesita una decisión (ver nota de la Fase 7 arriba) antes de poder conectarse sin pisar la vista de análisis de archivo existente. *(resuelto en la Fase 12 más abajo — sub-tab propio del panel derecho en vez de compartir contenedor)*

### ✅ Fase 11 — `radon` reemplazado por un sidecar Rust propio (`complexity-engine`)

No fue una reescritura por rendimiento — el motivo fue no querer depender de la lógica interna de una librería de terceros para algo que el proyecto puede tener propio, siendo un codebase que mantiene una sola persona. Medido antes de afirmar cualquier ganancia de velocidad, siguiendo el mismo criterio benchmark-primero que el trabajo de escalado a proyectos gigantes de arriba:

- [x] **Nuevo sidecar Rust `apps/complexity`** *(mudado a `services/complexity` después — ver [`CHANGELOG.md`](CHANGELOG.md))*, misma arquitectura que `terminal-server` (proceso persistente, HTTP, no subprocess por llamada ni extensión nativa PyO3) — `rustpython-parser` para el AST, código propio del proyecto para complejidad ciclomática (McCabe), Maintainability Index (fórmula Coleman-Oman) y métricas raw de líneas. Los dos sidecars ahora comparten un solo `Cargo.toml` en la raíz (2 `[[bin]]`, 1 `[lib]`).
- [x] **Benchmarkeado con Criterion contra `radon` real**, no asumido: 10 funciones — 0.42ms vs 8.97ms; 100 funciones — 4.7ms vs 89ms; 1000 funciones — 102ms vs 899ms. 9–21× más rápido, medido sobre los mismos archivos sintéticos en ambos lados.
- [x] `radon==6.0.1` eliminado de `requirements.txt`; `services/complexity_client.py` le pega al sidecar por HTTP y degrada con gracia (complejidad/MI vacíos, sin crash) si no está corriendo — mismo patrón de capacidad opcional que flake8/pylint.
- [x] Se arregló un bug real de condición de carrera encontrado al conectar esto: el diseño viejo cacheaba "¿está disponible la herramienta?" una sola vez al arrancar el backend, así que un primer `cargo build` lento podía dejar la capacidad trabada en `false` el resto de la sesión aunque el sidecar ya estuviera arriba. Los call sites reales del análisis ya no dependen de ese flag cacheado — le pegan al sidecar en vivo y degradan con gracia por pedido en vez de por sesión.
- [x] Tests unitarios en Rust (`cargo test`) para complejidad/MI/raw-metrics contra valores calculados a mano, corridos por el mismo job de CI que ya compilaba `terminal-server`.

### ✅ Fase 12 — Cerrados los últimos 3 clasificadores del CS Engine + ubicación del Problems Panel

Cierra los ítems de roadmap del CS Engine de la Fase 8 y la decisión pendiente del Problems Panel de la Fase 7/Fase 10:

- [x] **Regex → Chomsky Tipo-3 (Regular)**: detecta llamadas directas `re.compile/match/search/findall/...` por función. Honesto sobre su límite — no rastrea un `re.Pattern` guardado en variable, solo llamadas directas `re.XXX(...)`.
- [x] **Código con forma de gramática/parser → Chomsky Tipo-2 (Context-Free)**: la heurística exige *ambas* señales — nombre (`parse`/`grammar`/`tokenize`/`lexer`/...) *y* forma (recursión o patrón explícito de pila append/pop) — cualquiera de las dos sola generaba demasiados falsos positivos en las pruebas (un `factorial` recursivo plano no es un parser solo por ser recursivo).
- [x] **Recorrido de grafo → BFS/DFS/Orden Topológico, O(V+E)**: heurística sobre nombres de variable (`visited`/`seen`/`explored`, `in_degree`) más una cola (`.popleft()`) vs. forma de pila/recursión para distinguir BFS de DFS. Mismo estilo explícitamente heurístico, no-análisis-semántico, que el detector de WASM-hints ya existente (`_wasm_hints_python`) — el código nuevo sigue exactamente sus convenciones.
- [x] **El Problems Panel consiguió su propio lugar**: un 4to sub-tab del panel derecho (`Flujo · Análisis · Servidor · Problems`, `#rpp-problems`/`#problems-content`) en vez de intentar fusionarlo con la vista de análisis de archivo existente — resuelve el conflicto de contenedor DOM documentado desde la Fase 7 sin tocar el contenido más rico de esa vista (Pylint score, MI, tabla por función). Conectado en `editor.ts::applyMarkers()`, exactamente donde `panels/problems.ts` documentaba el punto de integración pensado desde que se escribió.
- [x] Eliminados 3 exports confirmados muertos, sin callers, re-verificados dos veces (`editor.ts::copyEditorContent`, `explorer.ts::explorerMarkModified`/`explorerRefresh`). Una lista más larga de exports sin caller *visible* se encontró en la misma pasada pero se dejó a propósito — no hay suficiente certeza de que no sean superficie de API para una feature que todavía no aterrizó (misma situación que tuvo el Force Graph antes de conectarse).

---

Las fases de abajo son los 11 pilares conceptuales de "Computer Science Intelligence" (ver arriba) más dos fases finales que quedan fuera de esa teoría — una arquitectónica, una de productización — agrupadas según qué tan fundamentadas están, no según qué tan lejos en el futuro caen. Las Fases 13–14 son la continuación más directa de lo que la Fase 8/12 ya shippeó; 15–17 son terreno teórico genuinamente nuevo; 18–20 construyen sobre la tabla de filosofía de lenguajes; 21–22 llenan un hueco que la pasada original de 9 pilares no vio (seguridad, calidad de código) — mismo estilo heurístico con confianza puntuada que todo lo demás acá, no una metodología nueva; 23 es arquitectónicamente distinta a todo lo de arriba (necesita un proceso corriendo, no solo texto fuente); 24–25 cierran el círculo — 24 es el límite de API que dejaría crecer las Fases 13–23 como plugins en vez de que cada analizador termine directo en `apps/api`, 25 es cómo el producto realmente llega a la gente una vez que hay algo para mostrar.

### ✅ Fase 13 — Algorithmic Intelligence *(extiende el motor O/Θ/Ω de la Fase 8, sin arquitectura nueva)*

La Fase 8 ya calcula O del peor caso, Ω del mejor caso, y Θ de cota ajustada por función — una aplicación específica y estándar de la notación asintótica en análisis de algoritmos (misma convención que usan CLRS y la mayoría de los textos de algoritmos). Vale la pena hacerlo explícito en vez de implícito, ya que las definiciones generales son más amplias que ese uso puntual:

- [x] **Referencia de notación asintótica**, mostrada donde ya aparece un resultado Big-O: una tabla `<details>` colapsable arriba de la tabla Big-O del panel Static (`panels/static.ts::_renderBigOTable`) cubre los cinco símbolos — O (cota superior), Ω (cota inferior), Θ (cota ajustada), o (cota superior estricta), ω (cota inferior estricta) — definición general primero, honesto en que `o`/`ω` no se calculan (no hay heurística estática confiable que distinga "estricto" del caso ajustado). El tooltip de hover en Python (`_build_hover_python`) recibe la misma explicación como nota de una línea debajo de su tabla O/Θ/Ω. El hover de JS/TS queda sin tocar a propósito — solo calcula O, no Θ/Ω, así que la referencia completa todavía no aplica ahí.
- [x] **Complejidad de espacio** junto a la de tiempo, construida en Rust y Python desde la misma pasada esta vez (`services/complexity/src/space.rs` + `static_parser.py::_infer_space_python`, las dos cableadas el mismo día) — mismo enfoque heurístico basado en AST que el motor de tiempo (sin ejecución), pero la señal es *qué estructura auxiliar se construye*, no *cuántas veces corre un loop*: un loop que solo acumula en un escalar (`total += x`) es O(1) de espacio aunque sea O(n) de tiempo. Detecta colecciones que crecen (`.append`/`.add`/`.update`/`.insert`/`.extend`, o asignación por subscript `d[k] = v`) según la profundidad de anidamiento de loop (O(n) en profundidad 1, O(n²) en profundidad 2 — una matriz construida en loops anidados), comprehensions anidadas (`[[0 for _ in range(n)] for _ in range(n)]`), y profundidad del call stack por recursión (O(log n) para recursión con división binaria, reusando el detector de split que ya tiene `bigo.rs`; O(n) para recursión lineal, porque Python no optimiza tail calls). Paridad byte a byte confirmada a mano entre las implementaciones Rust y Python en los 8 casos. Cableado en `/intel/hover` y `/intel/analyze` (Rust primero, fallback a Python, mismo gate que Big-O) y una columna **Space** nueva en la tabla Big-O del panel Static. Benchmarkeado con Criterion contra el tiempo real de `_parse_python()` sobre los mismos archivos sintéticos: 10 funciones — 0.77ms vs 18.0ms (23.4×); 100 — 9.17ms vs 212.6ms (23.2×); 1000 — 254ms vs 2083ms (8.2×) — y reportado con honestidad que agregar este pase hizo a `analyze_rich()` mismo 30–59% más lento contra su propio baseline previo (dos recorridos completos más por función se acumulan), no solo la ganancia contra Python.
- [x] **Reconocimiento de relaciones de recurrencia** para funciones divide-and-conquer, **solo en Rust** (`services/complexity/src/bigo.rs::resolve_master_theorem`) — deliberadamente no espejado al fallback de Python (ver abajo). Antes de esto, una función recursiva sin loop propio (`merge_sort`, una búsqueda binaria recursiva) atravesaba todas las ramas de la heurística de profundidad de loop existente y caía en su default genérico (`O(n)`, "caso base"), porque esa heurística solo razonaba sobre anidamiento de loops y un booleano `is_recursive` suelto. Reconoce `T(n) = a·T(n/2) + f(n)` y lo resuelve vía los tres casos del Teorema Maestro, mostrando la recurrencia misma (`T(n) = 2T(n/2) + Θ(n)`) junto al resultado `Θ(n log n)` — no solo la respuesta. `a` (subproblemas por llamada) es su propio razonamiento, no un contador reusado: trata las ramas de un mismo `if/elif/else` como alternativas mutuamente excluyentes (`max` entre ramas) y las statements secuenciales como aditivas (`sum`), así que las dos auto-llamadas incondicionales de `merge_sort` dan `a=2` mientras las dos llamadas de una búsqueda binaria recursiva, en ramas `elif`/`else` mutuamente excluyentes, dan correctamente `a=1` (`T(n)=T(n/2)+O(1)` → `Θ(log n)`, no `T(n)=2T(n/2)+O(1)` → `Θ(n)`). El grado de `f(n)` sale del máximo entre dos señales: la profundidad de loop propia de la función, o — vía un segundo pase sobre las funciones del archivo ya analizadas — el Big-O de un helper que llama (`merge_sort` llamando a `merge()`, ya resuelta por su cuenta a `O(n)`). 5 tests en Rust (`rich::tests`) cubren los casos reales (`merge_sort`, búsqueda binaria recursiva, recursión no-divide-and-conquer). **No portado a `static_parser.py`**: una primera pasada sí construyó paridad completa, pero con el sidecar Rust como camino principal (corre siempre que está disponible) y Python solo como fallback degradado, mantener dos implementaciones completas de esta pieza puntual no se justificaba — el fallback puro-Python ahora siempre reporta `recurrence: None` para toda función (confirmado por 4 tests en Python) y sigue respondiendo con la heurística genérica de loop/recursión de siempre, igual que antes de esta fase. Cableado en `/intel/hover` (solo la rama Rust-rich — la fila simplemente no aparece cuando el sidecar está caído) y en el tooltip del badge de recursión que ya existía en el panel Static — sin superficie de UI nueva, reusa el hover que ya estaba para `recursion_note`.

### 🟡 Fase 14 — Data Structures & Graph Intelligence *(mismo estilo heurístico que los detectores de WASM-hints/dead-code ya existentes)*

Continuación directa del CS Engine (Fase 8/12) — mismo enfoque de análisis estático, sin arquitectura nueva. No solo nombrar la estructura, explicarla:

- [ ] Detectar AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List a partir de la forma del AST
- [ ] Por cada detección: complejidad (tiempo *y* espacio), operaciones típicas, casos de uso, ventajas/desventajas contra las alternativas — el mismo estándar de "por qué", no solo "qué", que ya se exige el motor de Big-O
- [x] **Algoritmos de grafo como operaciones de primera clase, primera porción: centralidad/detección de hubs** (`routers/graph.py::_build_centrality_graph`) — nuevo tipo de grafo `centrality` que reusa el import graph que ya se construye para los otros 4 (Import/Call/Circular/Heatmap), puntuado con `nx.degree_centrality`/`in_degree`/`out_degree` sobre el mismo `DiGraph` de NetworkX que el detector de dependencias circulares ya construye — no una librería nueva, la misma que ya se ganaba su lugar. Un archivo se marca como "hub" cuando está entre los 5 con mayor in-degree *y* tiene al menos 2 dependientes (evita marcar como hub un archivo con una sola import entrante). Cableado en el dropdown de proyecto del panel Diagrama — alcanzable por un usuario, no solo una respuesta de API, la misma disciplina de "¿esto tiene un control de UI?" que enseñó el hueco del Force Graph. Deliberadamente todavía Python (`networkx`, no Rust) — el Graph Engine de la Fase 18 necesita la *construcción* del import/call graph en Rust primero, que todavía no existe; portar solo los algoritmos antes de eso significaría mantener dos representaciones de grafo separadas.
- [ ] **Detección de interacción entre estructuras anidadas** — ej. una hash table recorrida dentro de un loop que también toca esa misma hash table, marcada como un posible recorrido O(n²); análisis de complejidad que mira cómo las estructuras *se combinan*, no solo complejidad por función aislada

### 🔴 Fase 15 — Mathematical Intelligence *(matemática discreta usada para explicar programas, no un solver matemático)*

No es "Sythrall demuestra teoremas." Los conceptos de matemática discreta que ya sostienen el resto de este roadmap, hechos explícitos donde ya están implícitos en lo que Sythrall detecta:

- [ ] Conjuntos y relaciones — las operaciones de `dict`/`set` ya detectadas (el clasificador de recorrido de grafo de la Fase 8 lee `visited`/`seen` como conjuntos) enmarcadas explícitamente como operaciones de conjuntos, no solo heurísticas de nombre de variable
- [ ] Funciones — clasificación pura vs. con efectos secundarios (¿la función solo lee sus argumentos y retorna, o muta estado externo?), una propiedad estática real, no una heurística adivinada
- [ ] Combinatoria — la cardinalidad de loops anidados ya calculada para Big-O (la señal de profundidad de loop de la Fase 8) reencuadrada explícitamente como el conteo combinatorio que en realidad es
- [ ] Álgebra de Boole — hints de simplificación de De Morgan sobre condicionales complejos (`not (a and b)` ⟷ `(not a) or (not b)`), mostrados como nota de legibilidad, no como reescritura automática
- [ ] Framing de demostración por inducción sobre funciones recursivas que ya tienen caso base + paso inductivo detectados (el análisis de recursión de la Fase 8 ya encuentra ambos) — una nota explicativa, no una prueba generada

### 🔴 Fase 16 — Formal Language Intelligence *(completa la jerarquía de Chomsky que la Fase 8/12 ya empezó)*

La Fase 8/12 ya shippea dos niveles de esto: regex → Chomsky Tipo-3 (Regular), código con forma de gramática/parser → Chomsky Tipo-2 (Context-Free). Esta fase completa la jerarquía como referencia educativa y de clasificación:

- [ ] **Tipo-1 (Context-Sensitive) / Autómata Linealmente Acotado** y **Tipo-0 (Recursivamente Enumerable) / Máquina de Turing** — los dos niveles que faltan, agregados donde Sythrall encuentre una señal AST concreta para ellos, documentados con honestidad donde todavía no pueda
- [ ] **Panel de referencia de la Jerarquía de Chomsky** — cada tipo de gramática emparejado con el autómata que la reconoce (Regular → Autómata Finito, Context-Free → Autómata de Pila, Context-Sensitive → Autómata Linealmente Acotado, Tipo-0 → Máquina de Turing), enlazado desde donde ya dispara una clasificación regex/gramática hoy

### 🔬 Fase 17 — Compiler Intelligence *(integrar herramientas maduras, no reconstruirlas — todavía un spike de investigación, no comprometido)*

El framing de Cálculo Lambda ya existe sobre la recursión tail-call (Fase 8) — esta fase es donde esa teoría se conecta con un pipeline de compilador real, no un compilador propio de Sythrall:

- [ ] Visualización del pipeline del compilador (Lexer → AST → IR → optimización → codegen) — integrar [Compiler Explorer](https://godbolt.org) (open source) en vez de construir un compilador educativo desde cero
- [ ] Vista a nivel IR de la reescritura tail-call que la Fase 8 ya explica en prosa — mostrar cómo se ve el IR de una función tail-recursive reducida a un loop, en vez de solo afirmar que "podría" serlo

### 🟡 Fase 18 — Native Analysis Core *(migración comprometida: el rol de `static_parser.py` se muda a Rust, completo — no solo donde un benchmark lo favorezca)*

`complexity-engine` (Fase 11) fue la prueba de concepto; esta fase es la decisión que esa prueba justificó. **El motor de análisis estático se muda a Rust, completo** — parsing, construcción de AST, resolución de símbolos, complejidad, seguridad, grafos, métricas de calidad, todo, consolidado progresivamente en un core nativo modular. El rol de Python se acota a lo que el ML/DL Inspector de la Fase 2 ya hace bien: IA, ML, cargas científicas — no parsear código fuente. El estado final **no tiene capa de compatibilidad permanente ni doble implementación**: una vez que una pieza se porta y se prueba correcta, la versión Python que reemplazó se elimina, no se queda "por las dudas". `static_parser.py` en sí es el objetivo de la migración, no un fixture que la sobrevive.

Lo que no cambia: *cómo* se mueve cada porción sigue siendo disciplinado — se porta, se testea por paridad contra el Python que reemplaza sobre el mismo input, se benchmarkea con Criterion, y solo entonces se conecta a los call sites en vivo, exactamente el proceso que ya usó la primera porción de abajo. El compromiso es con el destino; el rigor está en cómo se llega a cada paso, para que migrar rápido nunca signifique migrar con descuido.

- [x] **Análisis Python rico, portado a `complexity-engine`, y el duplicado Python eliminado (no solo conectado en paralelo)** — el trabajo por función/clase/import de `_parse_python()` (Big-O, Θ/Ω, complejidad ciclomática, recursión tail-call, space complexity, security/taint, structural + naming smells, los 3 clasificadores del CS Engine) corre en Rust, expuesto como `POST /parse/python`. `_parse_python` pasó a ser `async` y ahora consulta el sidecar primero para todo esto — **las implementaciones heurísticas en Python se borraron, no se dejaron como fallback con paridad mantenida**: ~30 funciones eliminadas de `static_parser.py` (`_infer_big_o_python`/`_theta_omega_python`/`_loop_analysis_python`/`_has_binary_split_python`, `_recursion_info_python`, los clasificadores regex/grammar/graph-traversal, `_infer_space_python` + helpers, `_security_findings_python` + sus helpers de taint tracking, cada función `_check_*` de smells estructurales/naming). Sin el sidecar, el análisis de `.py` degrada a un esqueleto liviano solo-AST (`_skeleton_functions_python`/`_skeleton_classes_python`: nombre/línea/args/docstring/calls/complexity reales — pero `big_o`/`space_complexity` en `"?"`, `security_findings`/`structural_smells`/`naming_smells` en `[]`) en vez de una segunda implementación completa — la misma decisión que esta fase ya tomó para Halstead/MI (Fase 11) y Recurrence Relations (Fase 13), generalizada a todo el motor. **Ahora conectado también en `/static/parse` y `/static/parse-project`** (la tabla principal del panel Static y Project Health, antes 100% Python puro, sin siquiera chequear el sidecar) — el loop por archivo de `parse_project` usa `asyncio.gather` en vez de awaits secuenciales, así que N archivos son N round-trips concurrentes al sidecar, no N seriales. `pytest` ahora levanta el sidecar real para toda la sesión (`tests/conftest.py`, `cargo build --bin complexity-engine` si no existe el binario debug todavía) en vez de ejercitar un fallback Python que ya no existe — los ~130 tests de `test_security_findings.py`/`test_structural_smells.py`/`test_naming_smells.py`/`test_static_analysis.py` ahora verifican el motor Rust real de punta a punta vía HTTP, con un puñado de aserciones ajustadas donde el texto (en inglés) de Rust difiere del de la heurística Python (en español) ya eliminada.
- [ ] **Graph Engine** — construcción y recorrido de import/call graph para proyectos grandes, hoy Python puro en `graph.py`; la pieza que tanto `/static/parse` como el trabajo de algoritmos de grafo de la Fase 14 necesitan en Rust antes de poder dejar Python atrás
- [ ] **Dependency Engine** — detección de dependencias circulares y resolución cross-file a escala de proyecto — la otra mitad que `/static/parse` necesita, junto con WASM hints y detección de dead-code, para retirar del todo su path en Python
- [ ] **Symbol Engine** — go-to-definition / find-references sobre codebases grandes, hoy basado en regex/AST por archivo
- [ ] **Project Scanner** — el fan-out de recorrer+parsear archivos para análisis de proyecto completo (`read_project_files` y afines)
- [x] **Security, catálogo CWE v1 completo en los dos lenguajes** — `security.rs`: taint tracking + los 5 CWEs (SQL/Command Injection, Path Traversal, Deserialización insegura, credenciales hardcodeadas), portados el mismo día que se shippeó la versión Python para cada uno (Fase 21), misma disciplina de benchmarkear-y-después-swap que cada otra fila de acá
- [x] **Code Quality, cuatro porciones portadas, ahora todas Rust-only** — `maintainability.rs::halstead_metrics()`: los 5 componentes de Halstead (Fase 22), Rust-only a propósito desde que la Fase 11 reemplazó a `radon` (nunca existió fallback Python); `smells.rs`: 5 chequeos de smells estructurales, y `naming.rs`: 3 chequeos de smells de nombres — los dos se construyeron brevemente en ambos lenguajes, y el lado Python se eliminó en la misma pasada que retiró Big-O/space/security a Rust-only arriba, así que ahora comparten un solo límite en vez de dos distintos; scores de Project Health (Fase 22) que agregan `security_findings`/`structural_smells`/`naming_smells` a nivel de proyecto, agregación aritmética sobre resultados que el motor Rust-first ya calculaba, no lógica de análisis nueva. Los smells de arquitectura (Fase 22) también se shippearon, pero deliberadamente Python-only en `routers/graph.py` — el import graph cross-file sobre el que están construidos nunca fue portado a Rust (el ítem propio "Graph Engine" de esta fase, todavía sin abordar arriba), así que no había lado Rust al que portarlos todavía, el mismo tipo de límite que la excepción Rust-only de Halstead documenta al revés. El historial de tendencias por proyecto queda en cola
- [ ] **`static_parser.py` eliminado** — la meta real: una vez que cada endpoint que lo lee hoy (`/static/parse`, `/static/bigO`, `/intel/*`, `graph.py`) lea del core Rust en su lugar, el archivo Python se elimina, no se deja deprecado-pero-vivo

```
                    Sythrall
                       │
                 Native Analysis Core (Rust)
                       │
       ┌───────────────┼────────────────┐
       │               │                │
    Parsing          Analysis         Graph
   AST/símbolos  Complexity/Security  CFG/DFG
                       │
              Deep Analysis
       ┌───────────────┼───────────────┐
       │               │               │
    Security        Quality       Performance
```

Una cosa que esto deliberadamente NO es: `static_parser.py` → un solo `static_parser.rs` gigante. La división modular de arriba — módulos Rust separados por responsabilidad, el mismo patrón que ya usan `bigo.rs`/`classifiers.rs`/`recursion.rs`/`structure.rs` dentro de `rich.rs` hoy — es a donde porta cada ítem de arriba, no un port monolítico único. Y una vez que exista la Fase 24 (Extensibility Platform), este core tampoco tiene que cargar con todo para siempre — catálogos de CWE, reglas adicionales, soporte de lenguajes nuevos, y modelos de explicación de IA son exactamente el tipo de cosa que pertenece como plugin *encima* de estos primitives, no hardcodeado para siempre dentro del core mismo.

### 🔬 Fase 19 — Machine Intelligence *(el lado Assembly de la tabla de filosofía de lenguajes — integrar herramientas maduras, no reconstruirlas)*

Análisis de en qué *se convierte* el código, no solo qué dice. Cada uno de estos es su propio proyecto serio ya resuelto bien por herramientas open-source dedicadas — lo honesto es integrarlas, no reinventarlas:

- [ ] Soporte **Assembly (x86-64)** como lenguaje-objetivo — desglose de instrucciones/registros/control-flow a partir de snippets `.s`/asm inline pegados por el usuario *(pattern-matching sobre texto, no un disassembler)*
- [ ] Analizador de ejecutables (PE / ELF / Mach-O, secciones, imports/exports, símbolos) — envolver [Capstone](https://www.capstone-engine.org)/[LIEF](https://lief-project.github.io)/`objdump`, no escribir un disassembler a mano
- [ ] Explicadores de calling-convention y stack-frame ligados a la vista de Assembly una vez que exista — conecta la teoría (por qué el prólogo de una función se ve como se ve) con los bytes reales, cerrando el círculo desde la vista IR de la Fase 17 hasta el código máquina real

### 🔴 Fase 20 — Scientific Intelligence *(Fortran, más allá de una sola bala)*

Fortran como lenguaje-objetivo, conectado al stack numérico que Sythrall ya trae (los backends LAPACK/BLAS compilados de numpy/scipy) — no un lenguaje en el que el motor propio de Sythrall necesite estar escrito:

- [ ] Detección de loops `DO`/operaciones con arrays, candidatos a vectorización y SIMD
- [ ] Reconocimiento de algoritmos numéricos (operaciones con matrices, descomposiciones) con framing específico de dominio, ej. *"Multiplicación de matrices — O(n³), candidatos: SIMD, blocking, paralelización — dominio: HPC/Computación Numérica"* en vez de una etiqueta Big-O pelada
- [ ] Detección de uso de BLAS/LAPACK — marcar dónde un proyecto ya se apoya en backends numéricos compilados en vez de reimplementar algo que ya proveen

### ✅ Fase 21 — Security & Taint Intelligence *(análisis de flujo de datos basado en patrones, con confianza puntuada — no un reemplazo de SAST)*

Un hueco que la pasada original de 9 pilares no vio: nada en las Fases 13–20 mira seguridad. Continuación directa del estilo del CS Engine — los clasificadores de regex/grammar de la Fase 8/12 ya demuestran que "patrón heurístico + confianza etiquetada con honestidad" funciona — aplicado a flujo de datos source→sink en vez de forma algorítmica. Nada acá debería presentarse jamás como "esto ES una vulnerabilidad", solo como un patrón que merece la atención de una persona, con la evidencia mostrada para que la afirmación sea auditable. Catálogo CWE v1 completo:

- [x] **Taint tracking dentro de una sola función** — un recorrido recursivo que arrastra procedencia resuelve un valor hasta una fuente no confiable (`request.args`/`form`/`values`/`json`/`GET`/`POST`/`COOKIES`/`headers`, `input()`, `sys.argv`, `os.environ`/`os.getenv`) a través de asignaciones, concatenación de strings, f-strings y `.format()`, rastreando además *si* el taint se armó por construcción de string (la señal real de riesgo de SQLi/command injection) o solo pasó de largo. **Deliberadamente no cross-function** — el taint interprocedural necesita el call graph que el Dependency Engine de abajo todavía no construye. **Portado a Rust el mismo día que se shippeó en Python** (`services/complexity/src/security.rs`), no dejado como un "después". Ambos cubriendo la forma vulnerable y la forma segura/parametrizada de cada check, paridad byte a byte entre los dos confirmada a mano en cada caso.
- [x] **Catálogo CWE v1, 5 de 5 shippeados**: SQL Injection (CWE-89) — dispara solo cuando el query se *arma* por concatenación/f-string/`.format()`, así que `cursor.execute("...%s...", (val,))` correctamente no produce finding; Command Injection (CWE-78) — `os.system`/`os.popen` (siempre corren en shell) o `subprocess.*(..., shell=True)`, `subprocess.run([...])` sin `shell=True` correctamente no produce finding; credenciales hardcodeadas (CWE-798) — heurística de nombre+forma, a nivel de archivo en vez de por función porque es genuinamente ahí donde viven; **Path Traversal (CWE-22)** — `open(path)` necesita la misma señal "construido por concatenación" que SQLi (`open(f"uploads/{name}")` dispara, `open(config_path)` no), `os.path.join(base, seg)` dispara con cualquier segmento tainted más allá del primero incluso sin concatenar, porque unir un `"../etc/passwd"` tainted crudo ya es el traversal; **Deserialización insegura (CWE-502)** — `pickle.loads`/`pickle.load`/`marshal.loads` son inseguras por construcción (no existe una "forma segura", a diferencia de SQLi/command injection), así que disparan con confianza Media incluso *sin* taint provable y escalan a Alta cuando el argumento sí traza a una fuente de taint; `yaml.load` dispara salvo que se llame con `Loader=yaml.SafeLoader`/`CSafeLoader` explícito.
- [x] **Findings con confianza puntuada** (Alta/Media — Media introducida por el caso de CWE-502 sin taint pero igual riesgoso, el primer uso real de ese nivel), nunca un sí/no binario — refleja el precedente de "ambas señales requeridas" del clasificador de grammar de la Fase 12: una fuente sola no es un finding, fuente *y* la señal de concatenación/sink disparando juntas sí lo es
- [x] **Schema de finding en árbol de evidencia** — categoría → CWE → severidad → confianza → fuente → sink → línea → recomendación, una sola forma compartida para cada finding. Mostrado en el **panel Static** y en el **hover** de Python, Rust-only (`security.rs`, ver Fase 18 — la implementación de taint tracking en Python se eliminó, no se dejó como fallback; sin el sidecar, los findings degradan a lista vacía) — **todavía no en el Problems Panel**: ese tab se alimenta de markers de lint por cada tecla, y los findings de seguridad corren hoy sobre el mismo pase pesado `/static/parse` que ya usan los WASM hints, que viven en Static, no en Problems
- [x] **2 bugs reales encontrados y corregidos la misma semana que esto se shippeó**, ambos por una auditoría independiente, no autodetectados: (1) reasignar una variable tainted a un literal seguro (`cmd = request.args.get("x"); cmd = "ls -la"`) no limpiaba su taint, produciendo un falso positivo de confianza Alta sobre código defensivo ordinario; (2) una función anidada que reusaba el nombre de una variable externa filtraba taint entre scopes. Los dos se corrigieron haciendo que el recorrido de taint sea scope-aware (`_own_scope_nodes` en Python, `walk_stmts_own_scope` en Rust), con tests de regresión en ambos lenguajes. El port a Rust, construido *después* de los dos fixes, nunca tuvo ninguno de los dos bugs.
- [x] **Findings agregados a nivel de proyecto** (porción de la Fase 22) — `security_findings` de todos los archivos de un proyecto, no solo uno a la vez, mostrados en la vista de proyecto del panel Static con el archivo de origen por item, ordenados por severidad
- [ ] Explícitamente fuera de alcance, permanentemente: análisis de exploits completo (ROP, heap spray, use-after-free) — ya correctamente cercado bajo Execution Intelligence (Fase 23 de abajo) como "compite directamente con herramientas SAST maduras"; esta fase se queda dentro de ese mismo límite, solo que estática y más acotada

### ✅ Fase 22 — Code Quality Intelligence *(el Maintainability Index que la Fase 11 ya shippea, desglosado en sus partes auditables, más los smells que un solo número de MI no puede expresar)*

El Maintainability Index de `complexity-engine` (Fase 11) ya calculaba un Volumen de Halstead internamente pero solo lo exponía precocinado dentro de una fórmula. Esta fase muestra los componentes de los que esa fórmula está hecha, y agrega los smells estructurales/de nombres/arquitectura que un solo número de MI no puede capturar por sí solo:

- [x] **Métricas de Halstead, desglosadas** (`maintainability.rs::HalsteadMetrics`/`halstead_metrics()`) — Vocabulario (η1+η2), Longitud (N1+N2), Volumen, Dificultad, Esfuerzo, su propio struct junto al score de MI en vez de quedarse como un input opaco de una sola fórmula. Refactorizar `compute()` para que tome las métricas ya calculadas en vez de recorrer el suite de nuevo por su cuenta **eliminó** un recorrido duplicado de AST que existía antes de esta fase — una ganancia real medida (4.6–7.1% más rápido en 10/100 funciones), no solo superficie nueva. **Rust-only, sin fallback Python** — mismo límite que MI/CC/raw tienen desde que la Fase 11 reemplazó a `radon` directamente; el shape degradado es `halstead: null`, no un recálculo Python más lento.
- [x] **Smells estructurales** (`smells.rs`, Rust-only — ver Fase 18): función larga (LOC>50), exceso de parámetros (>5), anidamiento profundo (cualquier tipo de bloque, profundidad>4), clase grande (métodos>15 o LOC>300), god object (métodos≥20 Y atributos propios≥10) — umbrales convencionales de Fowler/Martin, cada smell trae su umbral y su razonamiento en el mensaje, nunca una etiqueta sola. La "lógica duplicada" (hashing de forma de AST) queda explícitamente diferida, no silenciada — necesita un esquema de normalización/hashing que todavía no existe.
- [x] **Dashboard de Project Health**: 4 scores (Security/Quality/Complexity/Architecture) agregados desde `security_findings`/`structural_smells`/complejidad/el detector de dependencias circulares a nivel de proyecto — cada score trae su fórmula y sus números crudos al lado, nunca un número pelado. Mostrado en el Dashboard y en la vista de proyecto del panel Static, compartiendo un solo renderer entre los dos.
- [x] **Smells de nombres** (`naming.rs`, Rust-only — ver Fase 18): variable de una sola letra fuera de un loop/comprehension (los parámetros quedan deliberadamente exentos — `def add(a, b)` es idiomático y el nombre está a la vista al lado de la firma, marcarlo sería ruido, no señal), mezcla de snake_case/camelCase dentro de un mismo archivo (hasta 3 ejemplos de cada estilo citados en el mensaje), una función anidada que liga un nombre ya usado en un scope que la contiene (un global del módulo o el parámetro/local de una función externa) — intencionalmente conservador, marca solo casos mecánicamente verificables en vez de juicios de "nombre poco claro" que necesitarían un LLM para arbitrar. Se cuenta aparte de los smells estructurales en el score de Quality (`health.quality.naming`), porque los problemas de nombres son de menor severidad por ocurrencia que un god object o un anidamiento profundo.
- [x] **Smells de arquitectura** (`routers/graph.py::_build_architecture_smells`, Python-only): coupling/cohesion por módulo, construido sobre el import graph que ya existe (Fase 6/14) — alto acoplamiento eferente (>15 imports internos, calibrado por encima de los 11 imports legítimos de composition root que `main.py` ya tiene), dependencia inestable (acoplamiento aferente ≥3 e inestabilidad Ce/(Ca+Ce) >0.5, métrica de Martin), y el detector de dependencias circulares ya shippeado reformulado como una instancia de un chequeo general de violación de capas (cada ciclo ahora también aparece como una entrada de smell `circular_dependency`, no solo en su propio grafo aparte). Python-only a propósito — el import graph cross-file que estos smells necesitan solo existe en `graph.py`, nunca portado a Rust (eso es el ítem propio "Graph Engine" de la Fase 18, deliberadamente no abordado acá) — el caso espejo de Halstead, que es Rust-only porque *sus* datos solo existen en Rust.

### 🔴 Fase 23 — Execution Intelligence *(instrumentación en runtime — un tipo de herramienta distinto a todo lo de arriba)*

Todo en las Fases 1–22 es análisis estático: texto fuente entra, hechos salen, sin necesidad de ejecución. Esta fase es arquitectónicamente distinta — necesita un proceso corriendo, ptrace/eBPF, o captura de paquetes en vivo, por eso se quedó como idea "de largo plazo" no comprometida durante mucho tiempo. Numerada acá para ser honestos de que es un destino real, no para afirmar que está cerca:

- [ ] Visualizador de memoria (stack/heap/data/bss) — requiere un proceso corriendo para inspeccionar, no texto fuente
- [ ] Analizador de concurrencia (race conditions, deadlocks, mal uso de mutex/atomic) — necesita ejecución real o herramientas como ThreadSanitizer, no inspección de AST
- [ ] Motor de SO (threads, paging, scheduling, IPC) — necesita tracing a nivel de kernel
- [ ] Analizador de redes (TCP/TLS/QUIC/WebSocket) — necesita captura de paquetes; esto es una herramienta con forma de Wireshark, no un analizador estático
- [ ] Analizador de seguridad más allá de detección de patrones (ROP, heap spray, use-after-free) — compite directamente con herramientas SAST maduras (Semgrep, CodeQL, Bandit); la versión realista se integra a la Fase 21 de arriba como "detectar el patrón + explicar el CWE", no un motor de análisis de exploits completo

### 🔴 Fase 24 — Extensibility Platform *(el límite de API que dejaría crecer las Fases 13–23 sin que cada feature termine en `apps/api` — una herramienta interna, no un marketplace público, hasta que haya demanda real de terceros)*

Cada fase de arriba se agrega directamente al código propio de Sythrall — razonable mientras lo mantiene una sola persona, pero cada una de las Fases 13–23 es realistamente del tamaño de un plugin por sí sola. Esta fase es el límite que dejaría que ese trabajo pase afuera del core sin que cada analizador termine siendo un archivo de `routers/` que Sythrall tiene que mantener para siempre. Deliberadamente acotada para una primera porción — sin marketplace, sin sandboxing, sin modelo de confianza para terceros — porque nada de eso tiene razón de existir antes de que un segundo plugin real, más allá de los que Sythrall mismo shippea, lo necesite de verdad:

- [ ] **Manifest de plugin + interfaz de capability** — un plugin declara qué analiza (`language`, `security`, `performance`, ...) y qué necesita (`ast`, `metrics`, `source`) en un manifest chico y tipado; los propios parsers de Python/JS/TS de Sythrall se vuelven las primeras implementaciones "built-in" de esa misma interfaz, probando que el límite es real antes de que un tercero lo toque
- [ ] **Un tipo de plugin shippeado de punta a punta** — la prueba concreta de si la interfaz es realmente usable, no un segundo sistema paralelo construido al lado. El trabajo de Fortran de la Fase 20 es el candidato natural: análisis numérico/científico como "plugin de lenguaje" en vez de otra rama hardcodeada en `static_parser.py`
- [ ] **Separación extension vs. plugin**, siguiendo una distinción que ya está implícita en cómo está organizado `apps/web` hoy: un *plugin* agrega un analizador (nuevos tipos de finding, nuevo lenguaje, nueva regla) y solo necesita la interfaz de capability de arriba; una *extension* agrega UI (un panel nuevo, de la misma forma en que el tab de Problems de la Fase 12 ya es uno) y consume la salida de un plugin sobre la misma forma JSON que ya lee cada panel — sin arquitectura nueva para las extensions, solo un nombre documentado para una costura que ya existe de manera informal
- [ ] **IA como capa de explicación opcional, nunca como el detector** — una interfaz `AIProvider` (ONNX/GGUF local, API remota, o ninguna configurada) que un plugin puede llamar para convertir `Evidence` (la ruta de flujo de datos de la Fase 21, el razonamiento de Big-O de la Fase 13) en prosa, con el finding determinista producido y completamente usable con cero IA configurada — la misma forma que ya tiene hoy el campo `reason` del Big-O, solo que opcionalmente pasado a un modelo en vez de solo strings de template
- [ ] Explícitamente diferido, sin fecha: registry/marketplace público, ejecución sandboxed/WASM de terceros, un SDK de plugins multi-lenguaje más allá de lo que el propio core de Sythrall ya usa — compromisos de infraestructura real que solo tienen sentido una vez que existan plugins construidos *para* Sythrall por alguien que no sea su autor, para justificarlos

La regla que sobrevive a todo lo de arriba: `apps/api` sigue funcionando con cero plugins instalados — siempre lo hizo, esta fase solo agrega una forma documentada de construir encima, nunca un requisito para hacerlo.

### 🔴 Fase 25 — Sythrall Platform *(cómo el producto llega a la gente, una vez que el CS Engine tiene algo para mostrar)*

Todo acá es ortogonal a las fases de teoría de arriba — trabajo de ingeniería/distribución que no depende de que las Fases 13–24 aterricen primero, cerrando el roadmap con cómo se usa Sythrall en vez de qué sabe:

- [ ] **Native Toolchain (Zig)** — build standalone (Zig, o PyInstaller/Nuitka + Tauri), un binario portable sin depender de Docker/Node/Python; cross-compilación para los binarios nativos que este proyecto ya shippea (`terminal-server`, `complexity-engine`), un solo toolchain en vez de matrices de CI por plataforma *(deliberadamente no compite con el rol de Rust en la Fase 18 — el trabajo de Zig es llevar a Sythrall hacia una máquina, no analizar lo que hay en ella)*
- [ ] **Integración Cython & WASM** — detección automática de candidatos Cython desde el análisis Big-O (funciones O(n²)+), generación de stubs `.pyx` desde firmas Python, compilación en Docker (MSVC/GCC), benchmark lado a lado Python-vs-Cython, speedup estimado en el hover provider, ruta de compilación WASM vía Emscripten
- [ ] **Execution Path Simulator** — vista animada tipo circuito del propio pipeline de análisis de Sythrall (`Input → Parser → AST → Dependency Resolver → Metrics → Report`), traza paso a paso con timing por etapa, exportable como SVG animado
- [ ] **Persistencia empresarial** — PostgreSQL + Delta Lake, historial de análisis, comparación de métricas entre versiones, autenticación JWT, API pública con rate limiting
- [ ] Extensión para VS Code, servidor LSP (el cliente natural para los hechos de las Fases 13–19 una vez que haya un protocolo estándar sirviéndolos), análisis de Jupyter Notebooks (`.ipynb`), integración ApexVision (`/analyze/image` con OpenCV + YOLOv11), dashboard de equipo con métricas agregadas
- [x] GitHub Action para CI/CD — `.github/workflows/ci.yml` (typecheck/lint/build/test en cada push/PR) + `release.yml` (tag → GitHub Release con notas del CHANGELOG + artefacto del build frontend)

### 🔬 Spikes de investigación — ideas sueltas, no comprometidas

- [x] ~~Extensión en Rust (PyO3) para el hot-path del parser estático~~ — **investigado dos veces con benchmarks reales, no adoptado ninguna de las dos**: la primera pasada perfiló `static_parser.py` con archivos *individuales* grandes (250+ funciones, 3000+ sintéticas) y encontró que la consolidación de recorridos AST ya hecha fue neutra (dentro del ruido de medición), con el parser ya rápido donde importa (~160ms para tamaños realistas). La segunda pasada (Fase 10, arriba) probó el otro eje — miles de *archivos* en un proyecto, no un archivo gigante — y encontró que el parser en sí seguía escalando lineal; el costo O(n²) real eran tres bugs de Python comunes en el código *alrededor* del parser, arreglados sin ningún lenguaje nuevo. Rust *sí* terminó entrando al proyecto (`terminal-server` de la Fase 9), pero para un problema genuinamente pensado para Rust — manejo de PTY cross-platform — no como reescritura de Python que ya funcionaba bien. Si algún día aparece un cuello de botella real en `parse_file` mismo, el modelo de integración sería un sidecar Axum (mismo patrón que `terminal-server`) hablando HTTP con FastAPI, no embeber vía PyO3 — más simple de mantener en solitario, sin matriz de builds de bindings nativos.
