use complexity_core::graph::{
    build_call_graph, build_centrality_graph, build_circular_graph, build_import_graph, CallEdgeInput,
    CallGraphFileInput, CallGraphFunctionInput, FileSummary, ImportRef,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Genera `n` archivos sintéticos encadenados (`f_i.py` importa `f_{i+1}.py`,
/// el último no importa nada) — mismo espíritu que `parse_bench.rs`, a nivel
/// de proyecto en vez de un solo archivo, que es la unidad real de trabajo
/// del Import Graph.
fn synthetic_project(n: usize) -> Vec<FileSummary> {
    (0..n)
        .map(|i| FileSummary {
            filename: format!("f_{i}.py"),
            language: "python".to_string(),
            functions: 3,
            imports: if i + 1 < n {
                vec![ImportRef { module: format!("f_{}", i + 1), line: 1 }]
            } else {
                Vec::new()
            },
            dead_code: 0,
        })
        .collect()
}

fn bench_build_import_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::graph::build_import_graph");
    for &n in &[10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(|| synthetic_project(n), |files| build_import_graph(black_box(files)), criterion::BatchSize::SmallInput);
        });
    }
    group.finish();
}

fn bench_build_centrality_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::graph::build_centrality_graph");
    for &n in &[10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || synthetic_project(n),
                |files| build_centrality_graph(black_box(files)),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Genera `n` archivos sintéticos, 1 función c/u, con una edge de llamada en
/// cadena (`f_i` llama a `f_{i+1}`, la última no llama a nadie) — mismo
/// espíritu de proyecto sintético que `synthetic_project`, pero con el shape
/// de entrada que necesita Call Graph (funciones + call_graph, no imports).
fn synthetic_call_graph_project(n: usize) -> Vec<CallGraphFileInput> {
    (0..n)
        .map(|i| CallGraphFileInput {
            filename: format!("f_{i}.py"),
            functions: vec![CallGraphFunctionInput {
                name: format!("f_{i}"),
                big_o: "O(n)".to_string(),
                complexity: 3,
                line: 1,
            }],
            call_graph: if i + 1 < n {
                vec![CallEdgeInput { from: format!("f_{i}"), to: format!("f_{}", i + 1) }]
            } else {
                Vec::new()
            },
        })
        .collect()
}

fn bench_build_call_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::graph::build_call_graph");
    for &n in &[10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || synthetic_call_graph_project(n),
                |files| build_call_graph(black_box(files)),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// `synthetic_project(n)` es una cadena acíclica — agrega un import de
/// `f_{n-1}.py` de vuelta a `f_0.py`, cerrando un único ciclo grande, para
/// medir el caso "hay al menos 1 ciclo real" y no solo el camino vacío.
fn synthetic_project_with_cycle(n: usize) -> Vec<FileSummary> {
    let mut files = synthetic_project(n);
    if let Some(last) = files.last_mut() {
        last.imports.push(ImportRef { module: "f_0".to_string(), line: 2 });
    }
    files
}

fn bench_build_circular_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("complexity_core::graph::build_circular_graph");
    for &n in &[10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::new("acyclic", n), &n, |b, &n| {
            b.iter_batched(|| synthetic_project(n), |files| build_circular_graph(black_box(files)), criterion::BatchSize::SmallInput);
        });
        group.bench_with_input(BenchmarkId::new("with_cycle", n), &n, |b, &n| {
            b.iter_batched(
                || synthetic_project_with_cycle(n),
                |files| build_circular_graph(black_box(files)),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_build_import_graph,
    bench_build_centrality_graph,
    bench_build_call_graph,
    bench_build_circular_graph
);
criterion_main!(benches);
