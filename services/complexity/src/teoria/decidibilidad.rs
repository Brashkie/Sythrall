//! **Decidibilidad: por qué un hallazgo dice `medium` y ninguna herramienta
//! puede hacer mejor.**
//!
//! Hoy la confianza de un hallazgo se comunica con `high`/`medium`, y eso se
//! lee como *"el análisis es flojo"*. Casi nunca es eso. La distinción real
//! es **binaria y sale del teorema de Rice**:
//!
//! - **Decidible** — se prueba leyendo. Dimensiones de matriz incompatibles,
//!   un modo octal `0666`, código inalcanzable desde el CFG, un `import imp`.
//!   Si el detector dice que pasa, pasa. No hay falsos positivos posibles
//!   salvo un bug del detector.
//! - **Aproximada** — la propiedad **no es decidible**, así que se aproxima
//!   de forma conservadora. Taint llegando a un sink, false sharing, un
//!   `.unwrap()` que quizá nunca falle. Que reporte `medium` no es una
//!   limitación de Sythrall: **es el límite del análisis estático**, y
//!   cualquier herramienta que prometa certeza ahí está mintiendo.
//!
//! **Por qué esto es una tabla y no un campo en 20 módulos.** La
//! decidibilidad es propiedad del *tipo* de hallazgo, no de cada instancia:
//! `division_by_zero` es decidible siempre, `potential_false_sharing` es
//! aproximado siempre. Una sola tabla la define, y la serialización de
//! `StructuralSmell` la agrega sola — ningún módulo emisor cambia.
//!
//! **Y el que no está clasificado se ve.** Un `kind` que no figure en la
//! tabla sale como `unclassified` en vez de callarse: agregar un hallazgo
//! nuevo sin decidir a qué lado pertenece queda visible, que es el mismo
//! criterio por el que `Nucleo` no tiene `Default`.

/// **Cuatro lados, y los dos últimos corrigen lo que este enum afirmaba.**
///
/// Hasta el 2026-10-16 acá decía, textual: *"los dos lados. No hay un
/// tercero: o se prueba leyendo, o no se puede"*. Lo había, y medirlo mostró
/// que eran **dos**, no uno (Fase 34, Línea E, paso 3 — ver `teoria::spark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decidibilidad {
    /// Se prueba leyendo el código. Sin falsos positivos posibles.
    Decidible,
    /// La propiedad no es decidible; esto es una aproximación conservadora.
    Aproximada,
    /// **Un probador externo la demostró.** No la probó este motor leyendo:
    /// la cerró Alt-Ergo o Z3 resolviendo una fórmula. Ponerla junto a
    /// `Decidible` sería adjudicarse una demostración ajena.
    DemostradaPorProbador,
    /// **El probador no pudo cerrarla**, y eso no es lo mismo que
    /// `Aproximada`. Aproximada significa que el análisis renuncia por un
    /// límite teórico y lo dice de antemano; esto es una obligación
    /// demostrable en principio que se quedó sin camino — con otra
    /// precondición, otro prover o más tiempo podría cerrarse.
    NoDemostrada,
}

impl Decidibilidad {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Decidibilidad::Decidible => "decidable",
            Decidibilidad::Aproximada => "approximate",
            Decidibilidad::DemostradaPorProbador => "machine-proved",
            Decidibilidad::NoDemostrada => "unproved",
        }
    }
}

use Decidibilidad::{Aproximada, Decidible, DemostradaPorProbador, NoDemostrada};

/// **La clasificación de cada tipo de hallazgo, con el motivo.**
///
/// El motivo no es decoración: es lo que convierte un `medium` de *"el
/// análisis es flojo"* a *"esto no se puede decidir, y acá está por qué"*.
const TABLA: &[(&str, Decidibilidad, &str)] = &[
    // ── SPARK: lo que probó OTRO, y lo que nadie pudo probar ───────────
    //
    // **Que el resultado viaje en el `kind` y no en un campo por instancia
    // es lo que deja intacto el diseño de este módulo.** La clasificación
    // sigue siendo función del TIPO de hallazgo: `spark_proved` es
    // demostrada siempre y `spark_unproved` es no-demostrada siempre. La
    // alternativa era meterle una excepción por instancia a la pieza más
    // simple del motor.
    (
        "spark_proved",
        DemostradaPorProbador,
        "discharged by an external prover (Alt-Ergo/Z3), not by reading the code",
    ),
    (
        "spark_unproved",
        NoDemostrada,
        "the prover ran out of road: provable in principle, not closed with the time and axioms it had",
    ),
    // ── Métricas de forma: se cuentan, no se infieren ──────────────────
    ("long_function", Decidible, "counting lines is exact"),
    ("excessive_parameters", Decidible, "counting parameters is exact"),
    ("deep_nesting", Decidible, "nesting depth is read straight off the tree"),
    ("large_class", Decidible, "counting methods and lines is exact"),
    ("god_object", Decidible, "counting methods and attributes is exact"),
    // ── Lógica: la verdad de la fórmula se decide sin ejecutar ─────────
    ("de_morgan_simplifiable", Decidible, "the equivalence is a theorem, not a guess"),
    ("contradictory_condition", Decidible, "propositional unsatisfiability is decidable"),
    ("tautological_condition", Decidible, "propositional validity is decidable"),
    ("redundant_condition", Decidible, "the implication is decidable on the formula"),
    ("duplicated_condition", Decidible, "textual identity modulo whitespace"),
    ("absorbable_condition", Decidible, "absorption is an algebraic law"),
    // ── Flujo de control: sale del CFG, que es exacto ──────────────────
    ("unreachable_code", Decidible, "unreachable from the control-flow graph"),
    ("undefined_state_transition", Decidible, "the transition table is read, not inferred"),
    ("unreachable_state", Decidible, "no edge reaches it in the declared table"),
    ("unhandled_state", Decidible, "the state has no arm in the table"),
    // ── Forma sintáctica exacta ────────────────────────────────────────
    ("mutable_default_argument", Decidible, "the default IS a mutable literal, visibly"),
    ("float_equality_comparison", Decidible, "`==` on floats is there or it is not"),
    ("recursive_template_no_base_case", Decidible, "no specialization exists in the file"),
    ("unsafe_block_without_safety_comment", Decidible, "the comment is there or it is not"),
    ("unsafe_fn_without_safety_doc", Decidible, "the `# Safety` section is there or not"),
    ("ts_ignore_instead_of_expect_error", Decidible, "which suppression was written is exact"),
    ("bare_type_ignore", Decidible, "the presence of an error code is exact"),
    ("double_type_assertion", Decidible, "`as unknown as T` is a syntactic shape"),
    ("sfinae_instead_of_concepts", Decidible, "`enable_if` in the signature is exact"),
    ("unwrap_in_fallible_fn", Decidible, "the signature returns Result and the call is `unwrap`"),
    ("catastrophic_cancellation_step", Decidible, "the step is a literal below the threshold"),
    // ── Interpretación abstracta: PROBADO, no estimado ─────────────────
    //
    // Los `condition_always_*` y los no-`possible_*` salen del dominio de
    // intervalos, que es una sobreaproximación: si el dominio dice que el
    // divisor ES cero en TODO el rango, eso está PROBADO. Por eso son
    // decidibles y sus gemelos `possible_*` no.
    ("condition_always_false", Decidible, "proved on the interval domain, not guessed"),
    ("condition_always_true", Decidible, "proved on the interval domain, not guessed"),
    ("division_by_zero", Decidible, "the divisor's whole range is zero"),
    ("index_out_of_range", Decidible, "the index's whole range is outside the array"),
    ("none_dereference", Decidible, "the value is None on every path reaching here"),
    // Fase 32 — tipos en lenguajes de bajo nivel. **Los seis son
    // decidibles, y no por casualidad**: un sistema de tipos ES una
    // aproximación decidible de la semántica, así que todo lo que se
    // responde dentro de él se prueba leyendo. Cuando el tipo no se puede
    // resolver —un `kind` simbólico sin definir— no se emite el hallazgo en
    // vez de emitirlo como aproximado.
    ("signed_unsigned_comparison", Decidible, "both operands' declared types are in the file"),
    ("implicit_precision_loss", Decidible, "declared width against the literal's own type"),
    ("int_pointer_cast", Decidible, "the cast is written in the source"),
    ("missing_implicit_none", Decidible, "the statement is present or it is not"),
    ("single_precision_literal_in_double", Decidible, "the literal's form and the declared kind are both textual"),
    ("integer_division_to_real", Decidible, "both operand types are declared in the same scope"),
    ("char_signedness_assumption", Decidible, "the declared type is plain `char` and the literal is negative"),
    // El cardinal del iterable esta escrito en la llamada: `permutations(xs)`
    // son n! sin ambiguedad. Que el problema SEA una asignacion o un
    // viajante no lo es — pero eso no es el hallazgo, es la sugerencia, y
    // por eso el mensaje presenta las dos salidas en vez de elegir.
    ("factorial_brute_force", Decidible, "the iterable's cardinality is written in the call itself"),
    // Gramaticas: los dos se resuelven contando referencias en el texto. El
    // silencio cuando hay reglas generadas por una funcion NO es una
    // aproximacion: es no emitir, que es distinto.
    ("undefined_grammar_rule", Decidible, "the reference is in the text and the rule set is closed"),
    ("unreferenced_grammar_rule", Decidible, "every reference in the grammar is countable"),
    // ── Aproximadas: la propiedad no es decidible ──────────────────────
    ("possible_division_by_zero", Aproximada, "zero is IN the range, not the whole range"),
    ("possible_index_out_of_range", Aproximada, "part of the range falls outside"),
    ("possible_none_dereference", Aproximada, "None is one of the possible values"),
    ("quadratic_list_membership", Aproximada, "the list may stay small; size is a runtime fact"),
    ("linear_op_in_loop", Aproximada, "the loop's trip count is a runtime fact"),
    ("unbounded_queue", Aproximada, "whether the producer outruns the consumer is a runtime fact"),
    ("unbounded_cache", Aproximada, "whether the key space is bounded is a runtime fact"),
    ("unbounded_task_spawn", Aproximada, "how many tasks are spawned is a runtime fact"),
    ("unbounded_convergence_loop", Aproximada, "whether it converges is undecidable (halting)"),
    ("deep_template_recursion", Aproximada, "the real depth depends on the instantiation"),
    ("dead_store", Aproximada, "`eval`/`locals()` could read it; conservative by design"),
    ("potential_false_sharing", Aproximada, "needs the real layout, a runtime and profiling"),
    ("overwritten_store", Aproximada, "`eval`/`locals()` could read it; exception edges are approximated"),
];

/// La decidibilidad de un tipo de hallazgo, y por qué.
pub fn clasificar(kind: &str) -> Option<(Decidibilidad, &'static str)> {
    TABLA.iter().find(|(k, _, _)| *k == kind).map(|(_, d, m)| (*d, *m))
}

/// La etiqueta que viaja en el JSON. **Un `kind` sin clasificar se ve**: sale
/// `unclassified` en vez de callarse o inventar un lado.
pub fn etiqueta_de(kind: &str) -> &'static str {
    match clasificar(kind) {
        Some((d, _)) => d.etiqueta(),
        None => "unclassified",
    }
}

/// El motivo, para que el panel pueda decir por qué.
pub fn motivo_de(kind: &str) -> Option<&'static str> {
    clasificar(kind).map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Los 44 tipos de smell que el motor emite hoy están clasificados.**
    /// La lista se toma del `SMELL_LABEL` del frontend, que es la que el
    /// panel ya tiene que mantener completa — si se agrega un hallazgo allá
    /// y no acá, este test lo dice.
    #[test]
    fn todos_los_smells_conocidos_estan_clasificados() {
        const CONOCIDOS: &[&str] = &[
            "long_function", "excessive_parameters", "deep_nesting", "large_class", "god_object",
            "quadratic_list_membership", "linear_op_in_loop", "de_morgan_simplifiable",
            "mutable_default_argument", "unreachable_code", "condition_always_false",
            "condition_always_true", "division_by_zero", "possible_division_by_zero",
            "index_out_of_range", "possible_index_out_of_range", "none_dereference",
            "possible_none_dereference", "contradictory_condition", "tautological_condition",
            "redundant_condition", "duplicated_condition", "absorbable_condition",
            "unbounded_queue", "unbounded_cache", "unbounded_task_spawn",
            "float_equality_comparison", "unbounded_convergence_loop",
            "catastrophic_cancellation_step", "undefined_state_transition", "unreachable_state",
            "unhandled_state", "recursive_template_no_base_case", "deep_template_recursion",
            "unwrap_in_fallible_fn", "ts_ignore_instead_of_expect_error", "double_type_assertion",
            "bare_type_ignore", "dead_store", "overwritten_store", "potential_false_sharing",
            "sfinae_instead_of_concepts", "unsafe_block_without_safety_comment",
            "unsafe_fn_without_safety_doc",
        ];
        let faltan: Vec<&str> =
            CONOCIDOS.iter().copied().filter(|k| clasificar(k).is_none()).collect();
        assert!(faltan.is_empty(), "sin clasificar: {faltan:?}");
        assert_eq!(CONOCIDOS.len(), 44);
        // Los dos de SPARK NO están en esta lista a propósito: no son smells
        // que el panel de hallazgos muestre, son obligaciones de prueba con
        // su propia sección. Están en la tabla igual porque la categoría es
        // de este módulo.
        assert!(clasificar("spark_proved").is_some());
        assert!(clasificar("spark_unproved").is_some());
    }

    /// **El par que resume el concepto entero.** `division_by_zero` y
    /// `possible_division_by_zero` salen del MISMO análisis y caen en lados
    /// opuestos: si el dominio de intervalos dice que el divisor es cero en
    /// todo su rango, está PROBADO; si el cero solo está adentro del rango,
    /// es una posibilidad. Mismo motor, distinta fuerza de la afirmación.
    #[test]
    fn el_mismo_analisis_da_los_dos_lados_segun_lo_que_pruebe() {
        assert_eq!(clasificar("division_by_zero").unwrap().0, Decidible);
        assert_eq!(clasificar("possible_division_by_zero").unwrap().0, Aproximada);
        assert_eq!(clasificar("none_dereference").unwrap().0, Decidible);
        assert_eq!(clasificar("possible_none_dereference").unwrap().0, Aproximada);
    }

    /// Lo aproximado lo es por una razón de RUNTIME, no por flojera del
    /// análisis, y el motivo tiene que decirlo.
    #[test]
    fn el_motivo_de_lo_aproximado_explica_que_falta_y_no_se_disculpa() {
        assert!(motivo_de("unbounded_convergence_loop").unwrap().contains("halting"));
        assert!(motivo_de("potential_false_sharing").unwrap().contains("profiling"));
        assert!(motivo_de("quadratic_list_membership").unwrap().contains("runtime fact"));
    }

    /// **Las dos categorías que este módulo decía que no existían.**
    /// `spark_proved` no es `Decidible` —lo probó otro— y `spark_unproved` no
    /// es `Aproximada` —no es una renuncia teórica, es una prueba que no
    /// cerró—. Si alguna cayera en los lados viejos, el `machine-proved`
    /// estaría adjudicándose una demostración ajena.
    #[test]
    fn spark_no_cae_en_ninguno_de_los_dos_lados_viejos() {
        assert_eq!(clasificar("spark_proved").unwrap().0, DemostradaPorProbador);
        assert_eq!(clasificar("spark_unproved").unwrap().0, NoDemostrada);
        assert_eq!(etiqueta_de("spark_proved"), "machine-proved");
        assert_eq!(etiqueta_de("spark_unproved"), "unproved");
        for k in ["spark_proved", "spark_unproved"] {
            let d = clasificar(k).unwrap().0;
            assert_ne!(d, Decidible, "{k} no lo probó este motor");
            assert_ne!(d, Aproximada, "{k} no es una aproximación conservadora");
        }
    }

    /// El motivo de cada una tiene que decir **quién** probó y **qué** faltó.
    #[test]
    fn el_motivo_de_spark_dice_quien_probo_y_que_falto() {
        assert!(motivo_de("spark_proved").unwrap().contains("external prover"));
        assert!(motivo_de("spark_unproved").unwrap().contains("ran out of road"));
    }

    /// Un `kind` nuevo sin clasificar **se ve**, no se cuela como decidible.
    #[test]
    fn un_kind_desconocido_sale_como_sin_clasificar() {
        assert_eq!(etiqueta_de("un_hallazgo_que_no_existe"), "unclassified");
        assert_eq!(etiqueta_de("long_function"), "decidable");
        assert_eq!(etiqueta_de("dead_store"), "approximate");
    }

    /// **La decidibilidad viaja en el JSON sin que ningún emisor la
    /// escriba.** Es la prueba de que el enfoque funciona: el smell se
    /// construye con los cuatro campos de siempre y sale con seis.
    #[test]
    fn la_decidibilidad_aparece_al_serializar_sin_tocar_al_emisor() {
        let s = crate::hallazgos::smells::StructuralSmell {
            kind: "division_by_zero",
            name: "f".to_string(),
            line: 3,
            message: "…".to_string(),
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["decidability"], "decidable");
        assert!(j["decidability_reason"].as_str().unwrap().contains("whole range"));
    }

    /// Y un `kind` sin clasificar sale `unclassified` **sin** motivo: no se
    /// inventa una explicación para algo que nadie decidió.
    #[test]
    fn un_kind_sin_clasificar_no_trae_motivo_inventado() {
        let s = crate::hallazgos::smells::StructuralSmell {
            kind: "todavia_no_clasificado",
            name: "f".to_string(),
            line: 1,
            message: "…".to_string(),
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["decidability"], "unclassified");
        assert!(j.get("decidability_reason").is_none());
    }

    /// Ningún tipo puede estar dos veces con lados distintos.
    #[test]
    fn no_hay_kinds_repetidos_en_la_tabla() {
        let mut vistos: Vec<&str> = TABLA.iter().map(|(k, _, _)| *k).collect();
        let total = vistos.len();
        vistos.sort_unstable();
        vistos.dedup();
        assert_eq!(vistos.len(), total, "hay un kind repetido en la tabla");
    }
}
