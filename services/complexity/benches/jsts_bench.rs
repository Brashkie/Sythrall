use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Mismo criterio de generador que `parse_bench.rs`/`complexity_bench.rs` —
/// escala n funciones con imports/exports/clases/loops mezclados, para que
/// el benchmark ejercite las 6 regexes de `jsts.rs` de una, no solo el caso
/// más simple (una sola función).
fn synthetic_source(n: usize) -> String {
    let mut src = String::from("import { useState } from 'react';\n\n");
    for i in 0..n {
        src.push_str(&format!(
            "export function f_{i}(a, b) {{\n  let total = 0;\n  for (let x = 0; x < a; x++) {{\n    for (let y = 0; y < b; y++) {{\n      if (x % 2 === 0) {{ total += x * y; }} else {{ total -= x; }}\n    }}\n  }}\n  return total;\n}}\n\nclass C_{i} extends Base_{i} {{}}\n\n"
        ));
    }
    src
}

fn bench_parse_js_ts(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::jsts::parse_js_ts");
    for &n in &[10usize, 100, 1000] {
        let source = synthetic_source(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &source, |b, src| {
            b.iter(|| complexity_core::jsts::parse_js_ts(black_box(src), black_box(false)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse_js_ts);
criterion_main!(benches);
