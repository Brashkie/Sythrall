// Fase 26 (Algorithm Validation Engine) — kernel de validación empírica en
// Zig, generalizando fortran_bench.rs (Fase 23) más allá de Fortran/matmul.
// Escrito por Sythrall, NUNCA código de usuario — mismo límite de seguridad
// que documenta fortran_bench.rs: superficie de ejecución fija, controlada
// al 100% por este módulo. Compilado y ejecutado por `zig_bench.rs`
// (`zig build-exe`) a varios tamaños de N para medir si un bubble sort
// escala como O(n²) de verdad, no solo por su forma estática (dos loops
// anidados).
const std = @import("std");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();

    // `std.process.args()` no soporta Windows directamente — hace falta la
    // variante con allocator explícito.
    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();
    _ = args.skip();
    const n_str = args.next() orelse return error.MissingArg;
    const n = try std.fmt.parseInt(usize, n_str, 10);

    const arr = try allocator.alloc(i64, n);
    defer allocator.free(arr);

    // PRNG determinista simple (xorshift-like) — mismos datos en cada
    // corrida, no aleatoriedad real del sistema.
    var seed: u64 = 12345;
    for (arr, 0..) |_, i| {
        seed = seed *% 6364136223846793005 +% 1;
        arr[i] = @as(i64, @intCast(seed % 1000));
    }

    var timer = try std.time.Timer.start();
    var i: usize = 0;
    while (i < n) : (i += 1) {
        var j: usize = 0;
        while (j < n - i - 1) : (j += 1) {
            if (arr[j] > arr[j + 1]) {
                const tmp = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = tmp;
            }
        }
    }
    const elapsed_ns = timer.read();
    const elapsed_s = @as(f64, @floatFromInt(elapsed_ns)) / 1_000_000_000.0;

    const stdout = std.io.getStdOut().writer();
    try stdout.print("{d}\n", .{elapsed_s});
}
