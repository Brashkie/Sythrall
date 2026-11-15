// ─────────────────────────────────────────────────────────────────────────────
//  Vocabulario compartido de hallazgos: como se LLAMA y de que color es cada
//  tipo de hallazgo que el motor emite.
//
//  Vivia adentro de `panels/static.ts`, que lo exportaba para que lo usaran
//  `panels/problems.ts`, `panels/dashboard.ts` y `panels/apis.ts`. Tres
//  paneles importando de un cuarto panel no es una dependencia, es un
//  accidente: el vocabulario no es del panel Static, es del producto.
// ─────────────────────────────────────────────────────────────────────────────

import type { ArchitectureSmell, NamingSmell, StructuralSmell } from '../api/client'

export function securitySeverityColor(s: string): string {
  return s === 'High' ? 'var(--err)' : s === 'Medium' ? 'var(--warn)' : 'var(--muted)'
}

export const SMELL_LABEL: Record<StructuralSmell['kind'], [string, string]> = {
  long_function: ['Función larga', 'var(--warn)'],
  excessive_parameters: ['Exceso de parámetros', 'var(--warn)'],
  deep_nesting: ['Anidamiento profundo', 'var(--warn)'],
  large_class: ['Clase grande', 'var(--orange)'],
  god_object: ['God object', 'var(--err)'],
  quadratic_list_membership: ['O(n²) oculto (list membership)', 'var(--orange)'],
  // La otra mitad del mismo problema: aquella es BÚSQUEDA lineal dentro de
  // un bucle, esta es MUTACIÓN lineal. Mismo naranja porque es el mismo
  // tipo de defecto — el código es cuadrático y no lo parece.
  linear_op_in_loop: ['O(n²) oculto (mutación)', 'var(--orange)'],
  de_morgan_simplifiable: ['Simplificable (De Morgan)', 'var(--info)'],
  // Naranja, no amarillo: a diferencia de los smells de tamaño/anidamiento
  // (que son de mantenibilidad), un default mutable cambia el
  // COMPORTAMIENTO entre llamadas — está más cerca del O(n²) oculto que de
  // "esta función es larga".
  mutable_default_argument: ['Default mutable compartido', 'var(--orange)'],
  // Rojo, no naranja: los demas smells son de mantenibilidad ("esto se lee
  // mal"); codigo inalcanzable es un DEFECTO — o sobra, o el flujo no es el
  // que el autor cree. Sale del grafo de flujo, no de un heuristico.
  unreachable_code: ['Codigo inalcanzable', 'var(--err)'],
  // Rojo tambien: una guarda que nunca puede ser verdadera es un defecto, no
  // una cuestion de estilo. Sale de interpretacion abstracta (`intervals.rs`),
  // no de un patron sintactico.
  condition_always_false: ['Condicion siempre falsa', 'var(--err)'],
  condition_always_true: ['Condicion siempre verdadera', 'var(--err)'],
  division_by_zero: ['Division por cero', 'var(--err)'],
  // Naranja y no rojo: falla para ALGUN valor alcanzable, no para todos —
  // la diferencia entre "esto siempre revienta" y "el guard no cubre el 0".
  possible_division_by_zero: ['Posible division por cero', 'var(--orange)'],
  // Mismo par que division/posible-division, y por el mismo motivo: uno
  // esta probado con el largo del literal y el rango del indice, el otro
  // falla solo para algunos de los valores que el indice puede tomar.
  index_out_of_range: ['Indice fuera de rango', 'var(--err)'],
  possible_index_out_of_range: ['Posible indice fuera de rango', 'var(--orange)'],
  // Tercer par de la misma familia. El probado es el `x = None` seguido de
  // `x.algo`; el posible es el caso que cruza funciones — una que devuelve
  // None en una rama y un llamador que no lo chequea.
  none_dereference: ['Uso de None', 'var(--err)'],
  possible_none_dereference: ['Posible uso de None', 'var(--orange)'],
  // Logica proposicional. Los dos primeros dejan codigo muerto probado
  // (el cuerpo nunca corre, o el `else` nunca corre), asi que van en rojo
  // con el resto de los defectos. Los otros tres no matan nada: la
  // condicion sobra, que es otra cosa.
  contradictory_condition: ['Condicion contradictoria', 'var(--err)'],
  tautological_condition: ['Condicion tautologica', 'var(--err)'],
  redundant_condition: ['Condicion redundante', 'var(--orange)'],
  duplicated_condition: ['Condicion duplicada', 'var(--orange)'],
  absorbable_condition: ['Condicion absorbible', 'var(--orange)'],
  // Teoria de colas. Naranja y no rojo a proposito: que falte la cota es
  // un hecho probado, pero que la cola CREZCA depende de la carga (ley de
  // Little), y eso el codigo no lo dice. Se reporta la cota que falta, no
  // un leak.
  unbounded_queue: ['Cola sin cota', 'var(--orange)'],
  unbounded_cache: ['Cache sin cota', 'var(--orange)'],
  unbounded_task_spawn: ['Concurrencia sin cota', 'var(--orange)'],
  // Calculo numerico. La igualdad de flotantes va en rojo: `x == 0.1` es
  // falso para un valor matematicamente igual, y eso es un defecto, no un
  // riesgo. Los otros dos dependen de datos que el codigo no dice.
  float_equality_comparison: ['Igualdad de flotantes', 'var(--err)'],
  unbounded_convergence_loop: ['Convergencia sin tope', 'var(--orange)'],
  catastrophic_cancellation_step: ['Paso demasiado chico', 'var(--orange)'],
  // Automatas. El primero va en rojo porque no descansa en ninguna
  // suposicion: todas las claves de la tabla estan a la vista, asi que un
  // destino que no es clave es un KeyError probado. Los otros dos si
  // dependen de que el alcance analizado sea todo lo que hay.
  undefined_state_transition: ['Transicion a estado inexistente', 'var(--err)'],
  unreachable_state: ['Estado inalcanzable', 'var(--orange)'],
  unhandled_state: ['Estado sin atender', 'var(--orange)'],
  // Fase 34, Linea A — costo de COMPILACION, no de runtime. El primero va
  // en rojo porque una recursion de templates sin caso base no es lento:
  // es un build que falla. El segundo es costo, y va en naranja.
  recursive_template_no_base_case: ['Recursion de templates sin caso base', 'var(--err)'],
  deep_template_recursion: ['Recursion de templates profunda', 'var(--orange)'],
  // Amarillo, no rojo: un `unsafe` sin justificar NO es un defecto — el
  // codigo puede ser perfectamente correcto. Lo que falta es la prueba
  // escrita de por que lo es, que es justo lo que hace revisable a la
  // superficie. Marcarlo en rojo diria que `unsafe` esta mal, y no lo esta.
  // Rojo, no amarillo, y la diferencia con los dos de arriba es real: un
  // `unsafe` sin justificar puede ser correcto y solo faltarle la prueba
  // escrita. Acá la alternativa EXISTE y no se tomó — la firma ya promete
  // un `Result`, y `?` lo propaga en un carácter.
  unwrap_in_fallible_fn: ['unwrap evitable (había `?`)', 'var(--err)'],
  // Amarillo: `@ts-ignore` puede ser necesario; lo que le falta es la
  // forma que se autolimpia (`@ts-expect-error`).
  ts_ignore_instead_of_expect_error: ['@ts-ignore en vez de @ts-expect-error', 'var(--warn)'],
  // Rojo: apaga el ultimo chequeo que TS hace sobre una asercion.
  double_type_assertion: ['Asercion doble (as unknown as)', 'var(--err)'],
  // Amarillo, como `@ts-ignore`: puede ser necesario; lo que le falta es
  // la forma que sigue avisando por los demas errores.
  bare_type_ignore: ['type: ignore sin codigo de error', 'var(--warn)'],
  // Amarillo: casi siempre es codigo que sobro de un refactor, no un
  // defecto de comportamiento. Lo que cuesta es leerlo.
  dead_store: ['Asignacion muerta (nunca se lee)', 'var(--warn)'],
  overwritten_store: ['Escritura pisada antes de leerse', 'var(--warn)'],
  // Naranja, no rojo: el programa es CORRECTO (son atomicos, no hay data
  // race). Lo que se pierde es el paralelismo, y ademas el hallazgo es
  // POTENCIAL — probarlo exige layout real, runtime y profiling.
  potential_false_sharing: ['False sharing potencial (linea de cache)', 'var(--orange)'],
  // Azul informativo, no advertencia: con C++17 o anterior `enable_if` es
  // la unica forma, y el motor no sabe a que estandar apunta el proyecto.
  sfinae_instead_of_concepts: ['SFINAE donde hay concepts (C++20)', 'var(--info)'],
  // Fase 32 — tipos en lenguajes de bajo nivel. Rojo los tres que cambian el
  // VALOR calculado en silencio; naranja los que son de forma o de habito.
  signed_unsigned_comparison: ['Mezcla signo/sin signo', 'var(--err)'],
  implicit_precision_loss: ['Perdida de precision implicita', 'var(--err)'],
  int_pointer_cast: ['Entero <-> puntero', 'var(--orange)'],
  missing_implicit_none: ['Falta implicit none', 'var(--orange)'],
  single_precision_literal_in_double: ['Literal simple en variable doble', 'var(--err)'],
  integer_division_to_real: ['Division entera asignada a real', 'var(--err)'],
  char_signedness_assumption: ['char sin signo definido', 'var(--err)'],
  factorial_brute_force: ['Fuerza bruta factorial', 'var(--err)'],
  undefined_grammar_rule: ['Regla de gramatica inexistente', 'var(--err)'],
  unreferenced_grammar_rule: ['Regla de gramatica sin usar', 'var(--orange)'],
  unsafe_block_without_safety_comment: ['unsafe sin comentario SAFETY', 'var(--warn)'],
  unsafe_fn_without_safety_doc: ['unsafe fn sin seccion # Safety', 'var(--warn)'],
}

// Ni structural_smells ni naming_smells traen un campo `severity` propio (a
// diferencia de SecurityFinding) — el color ya asignado arriba/abajo por
// `kind` es el único juicio de gravedad que el sistema ya expresa en algún
// lado, así que el Dashboard lo reusa para el widget "Findings by Severity"
// en vez de inventar una escala nueva sin verla reflejada en ningún otro lado.
export function smellColorSeverity(color: string): 'High' | 'Medium' | 'Low' {
  if (color === 'var(--err)') return 'High'
  if (color === 'var(--orange)') return 'Medium'
  return 'Low'
}

export const NAMING_SMELL_LABEL: Record<NamingSmell['kind'], [string, string]> = {
  shadowed_name: ['Nombre tapado', 'var(--err)'],
  inconsistent_casing: ['Casing inconsistente', 'var(--orange)'],
  single_letter_name: ['Nombre de una letra', 'var(--warn)'],
}

export const ARCH_SMELL_LABEL: Record<ArchitectureSmell['kind'], [string, string]> = {
  circular_dependency: ['Dependencia circular', 'var(--err)'],
  unstable_dependency: ['Dependencia inestable', 'var(--orange)'],
  high_efferent_coupling: ['Alto acoplamiento eferente', 'var(--warn)'],
}
