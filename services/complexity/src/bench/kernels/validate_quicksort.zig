// Fase 26 (Algorithm Validation Engine) — kernel de validación empírica en
// Zig, cuarto uso de Zig en este motor (bubble sort en zig_bench.rs: O(n²);
// BFS en bfs_bench.rs: O(V+E); agregación en agg_bench.rs: O(n)) — y el
// primero que demuestra recursión en Zig dentro de este motor. Escrito por
// Sythrall, NUNCA código de usuario — mismo límite de seguridad que
// documentan el resto de los kernels de esta fase.
//
// Quicksort recursivo (partición de Lomuto, último elemento como pivot) —
// misma forma O(n log n) que `mergesort_bench.rs` (Assembly, bottom-up
// iterativo) ya valida, pero por una técnica genuinamente distinta: divide
// y vencerás recursivo en vez de fusión iterativa. Esto es explícitamente
// lo que el ROADMAP nombraba como pendiente ("a recursive sorting shape").
//
// Valores pseudo-aleatorios en un rango ancho (mil millones), NO acotados a
// unos pocos cientos como `validate_bubble_sort.zig` — con un rango chico,
// un `n` de varios cientos de miles generaría muchísimos duplicados, y la
// partición de Lomuto se degrada hacia O(n²) en presencia de duplicados
// masivos (el problema clásico de la "bandera holandesa"). Un rango ancho
// hace que los duplicados sean estadísticamente despreciables para los
// tamaños que este kernel usa, preservando el caso promedio O(n log n)
// que el pivot "último elemento" logra sobre datos verdaderamente
// aleatorios (su caso patológico es específicamente entrada YA ordenada,
// que no es lo que se genera acá).
//
// Verifica que el resultado quede realmente ordenado antes de reportar un
// tiempo — un kernel roto no debe alimentar en silencio un número
// plausible al ajuste de exponente empírico (mismo criterio que
// `validate_mergesort_driver.c` ya estableció): imprime `-1` si no.
const std = @import("std");

fn partition(arr: []i64, low: usize, high: usize) usize {
    const pivot = arr[high];
    var i = low;
    var j = low;
    while (j < high) : (j += 1) {
        if (arr[j] < pivot) {
            const tmp = arr[i];
            arr[i] = arr[j];
            arr[j] = tmp;
            i += 1;
        }
    }
    const tmp = arr[i];
    arr[i] = arr[high];
    arr[high] = tmp;
    return i;
}

fn quicksort(arr: []i64, low: usize, high: usize) void {
    if (low >= high) return;
    const p = partition(arr, low, high);
    if (p > 0) quicksort(arr, low, p - 1);
    quicksort(arr, p + 1, high);
}

fn is_sorted(arr: []const i64) bool {
    var i: usize = 1;
    while (i < arr.len) : (i += 1) {
        if (arr[i - 1] > arr[i]) return false;
    }
    return true;
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();

    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();
    _ = args.skip();
    const n_str = args.next() orelse return error.MissingArg;
    const n = try std.fmt.parseInt(usize, n_str, 10);

    const arr = try allocator.alloc(i64, n);
    defer allocator.free(arr);

    // PRNG determinista simple (xorshift-like), mismo criterio que
    // validate_bubble_sort.zig/validate_graph_bfs.zig — mismos datos en
    // cada corrida, no aleatoriedad real del sistema.
    var seed: u64 = 12345;
    for (arr, 0..) |_, i| {
        seed = seed *% 6364136223846793005 +% 1;
        arr[i] = @as(i64, @intCast(seed % 1_000_000_000));
    }

    var timer = try std.time.Timer.start();
    if (n > 0) quicksort(arr, 0, n - 1);
    const elapsed_ns = timer.read();
    const elapsed_s = @as(f64, @floatFromInt(elapsed_ns)) / 1_000_000_000.0;

    const stdout = std.io.getStdOut().writer();
    if (!is_sorted(arr)) {
        try stdout.print("-1\n", .{});
        return;
    }
    try stdout.print("{d}\n", .{elapsed_s});
}
