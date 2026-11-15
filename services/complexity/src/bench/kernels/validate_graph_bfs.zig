// Fase 26 (Algorithm Validation Engine) — segundo kernel de validación en
// Zig, generalizando `validate_bubble_sort.zig` a una forma algorítmica
// genuinamente distinta: recorrido de grafos (BFS), no ordenamiento ni
// aritmética. Escrito por Sythrall, NUNCA código de usuario — mismo límite
// de seguridad que el resto de Fase 26/23: superficie de ejecución fija,
// controlada al 100% por este módulo. Compilado y corrido por
// `graph_bench.rs` a varios tamaños de N (número de vértices) para medir si
// un BFS sobre un grafo con grado de salida fijo escala como O(V) de verdad.
const std = @import("std");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();

    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();
    _ = args.skip();
    const n_str = args.next() orelse return error.MissingArg;
    const n = try std.fmt.parseInt(usize, n_str, 10);

    const degree: usize = 4;

    // Grafo disperso determinista: cada vertice tiene `degree` aristas
    // salientes. La primera arista arma un anillo (i -> i+1 mod n) que
    // garantiza que TODO vertice es alcanzable desde el 0 -- sin esto, BFS
    // podria terminar temprano si algun vertice queda desconectado, dando
    // una medicion que no refleja realmente O(V+E).
    const neighbors = try allocator.alloc(usize, n * degree);
    defer allocator.free(neighbors);
    for (0..n) |i| {
        neighbors[i * degree + 0] = (i + 1) % n;
        var d: usize = 1;
        while (d < degree) : (d += 1) {
            neighbors[i * degree + d] = (i * (7 + d * 13) + d * 5 + 1) % n;
        }
    }

    const visited = try allocator.alloc(bool, n);
    defer allocator.free(visited);
    @memset(visited, false);

    const queue = try allocator.alloc(usize, n);
    defer allocator.free(queue);

    var timer = try std.time.Timer.start();

    var head: usize = 0;
    var tail: usize = 0;
    visited[0] = true;
    queue[tail] = 0;
    tail += 1;
    var visited_count: usize = 1;

    while (head < tail) {
        const u = queue[head];
        head += 1;
        var d: usize = 0;
        while (d < degree) : (d += 1) {
            const v = neighbors[u * degree + d];
            if (!visited[v]) {
                visited[v] = true;
                queue[tail] = v;
                tail += 1;
                visited_count += 1;
            }
        }
    }

    const elapsed_ns = timer.read();
    const elapsed_s = @as(f64, @floatFromInt(elapsed_ns)) / 1_000_000_000.0;

    const stdout = std.io.getStdOut().writer();
    try stdout.print("{d} {d}\n", .{ elapsed_s, visited_count });
}
