// Fase 26 (Algorithm Validation Engine) — kernel de validación empírica en
// Zig, tercer uso de Zig en este motor (bubble sort en zig_bench.rs, BFS en
// bfs_bench.rs). Este es específicamente la pieza 2 de "migrar de
// numpy/pandas/scikit-learn" que el usuario pidió (2026-08-31): antes de
// proponer un reemplazo nativo para un `groupby().agg()` de pandas, mide un
// kernel equivalente escrito por Sythrall (nunca código de usuario) y
// compara contra pandas real — ver agg_bench.rs y
// routers/execution.py::validate_aggregation_vs_pandas.
//
// Agregación por categoría (suma y promedio), 10 categorías fijas — a
// propósito NO usa un hash map: con categorías fijas 0..9, dos arreglos de
// tamaño 10 (sums/counts) bastan, sin la complejidad de una tabla hash real.
// Esto es intencional: el "shape" que se valida es el recorrido lineal de
// agregación (O(n)), no una implementación de hashing.
const std = @import("std");

const N_CATEGORIES: usize = 10;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();

    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();
    _ = args.skip();
    const n_str = args.next() orelse return error.MissingArg;
    const n = try std.fmt.parseInt(usize, n_str, 10);

    const categories = try allocator.alloc(u8, n);
    defer allocator.free(categories);
    const values = try allocator.alloc(i64, n);
    defer allocator.free(values);

    // Datos deterministas — mismos en cada corrida, sin aleatoriedad real
    // del sistema (mismo criterio que validate_bubble_sort.zig).
    var i: usize = 0;
    while (i < n) : (i += 1) {
        categories[i] = @intCast(i % N_CATEGORIES);
        values[i] = @as(i64, @intCast((i *% 37 +% 11) % 1000));
    }

    var sums = [_]i64{0} ** N_CATEGORIES;
    var counts = [_]i64{0} ** N_CATEGORIES;

    var timer = try std.time.Timer.start();
    i = 0;
    while (i < n) : (i += 1) {
        const cat = categories[i];
        sums[cat] += values[i];
        counts[cat] += 1;
    }
    // Promedio por categoría — O(N_CATEGORIES), constante, no distorsiona
    // la medición del recorrido O(n) de arriba.
    var means: [N_CATEGORIES]f64 = undefined;
    var c: usize = 0;
    while (c < N_CATEGORIES) : (c += 1) {
        means[c] = if (counts[c] > 0) @as(f64, @floatFromInt(sums[c])) / @as(f64, @floatFromInt(counts[c])) else 0.0;
    }
    const elapsed_ns = timer.read();
    const elapsed_s = @as(f64, @floatFromInt(elapsed_ns)) / 1_000_000_000.0;

    // Usa `means` para que el compilador no elimine el cálculo por dead-code.
    var checksum: f64 = 0.0;
    for (means) |m| checksum += m;
    std.mem.doNotOptimizeAway(&checksum);

    const stdout = std.io.getStdOut().writer();
    try stdout.print("{d}\n", .{elapsed_s});
}
