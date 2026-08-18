use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Mismo generador que `complexity_bench.rs` — funciones con regex, recursión
/// y un loop, para que `analyze_rich` ejercite los 3 clasificadores nuevos
/// además de Big-O/CC, no solo el caso más simple.
fn synthetic_source(n: usize) -> String {
    let mut src = String::from("import re\n\n");
    for i in 0..n {
        src.push_str(&format!(
            "def f_{i}(x, y):\n    total = 0\n    if re.match(r'\\d+', str(x)):\n        total += 1\n    for a in range(x):\n        if a % 2 == 0 and y > 0:\n            total += a\n        elif a % 3 == 0 or y < 0:\n            total -= a\n        else:\n            total *= 1\n    return total\n\n"
        ));
    }
    src
}

fn bench_analyze_rich(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::analyze_rich");
    for &n in &[10usize, 100, 1000] {
        let source = synthetic_source(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &source, |b, src| {
            b.iter(|| complexity_core::analyze_rich(black_box(src)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_analyze_rich);
criterion_main!(benches);
