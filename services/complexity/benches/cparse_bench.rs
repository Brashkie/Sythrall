use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Mismo criterio de generador que `jsts_bench.rs`/`parse_bench.rs` — escala
/// n funciones con loops anidados + includes + structs, para ejercitar el
/// recorrido tree-sitter completo (`function_definition`/`preproc_include`/
/// `struct_specifier`/`call_expression`), no solo una función suelta.
fn synthetic_source(n: usize) -> String {
    let mut src = String::from("#include <stdio.h>\n#include <stdlib.h>\n\n");
    for i in 0..n {
        src.push_str(&format!(
            "struct Point_{i} {{ int x; int y; }};\n\nint f_{i}(int a, int b) {{\n    int total = 0;\n    for (int x = 0; x < a; x++) {{\n        for (int y = 0; y < b; y++) {{\n            if (x % 2 == 0) {{ total += do_work(x, y); }} else {{ total -= x; }}\n        }}\n    }}\n    return total;\n}}\n\n"
        ));
    }
    src
}

fn bench_parse_c(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::cparse::parse_c");
    for &n in &[10usize, 100, 1000] {
        let source = synthetic_source(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &source, |b, src| {
            b.iter(|| complexity_core::cparse::parse_c(black_box(src)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse_c);
criterion_main!(benches);
