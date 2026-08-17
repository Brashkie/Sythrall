use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Genera un archivo Python sintético con `n` funciones, cada una con un
/// puñado de ramas — mismo espíritu que el harness de proyectos sintéticos
/// usado en v4.6 para medir el import graph, pero a nivel de un solo archivo,
/// que es la unidad real de trabajo de `radon`/este motor.
fn synthetic_source(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!(
            "def f_{i}(x, y):\n    total = 0\n    for a in range(x):\n        if a % 2 == 0 and y > 0:\n            total += a\n        elif a % 3 == 0 or y < 0:\n            total -= a\n        else:\n            total *= 1\n    return total\n\n"
        ));
    }
    src
}

fn bench_analyze(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::analyze");
    for &n in &[10usize, 100, 1000] {
        let source = synthetic_source(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &source, |b, src| {
            b.iter(|| complexity_core::analyze(black_box(src)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_analyze);
criterion_main!(benches);
