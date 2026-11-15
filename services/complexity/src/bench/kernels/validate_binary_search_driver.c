/* Fase 26 (Algorithm Validation Engine) — driver C mínimo para el kernel de
 * Assembly (`validate_binary_search.s`), mismo motivo que los otros drivers
 * de esta fase: un objeto `.s` puro no trae su propio entry point. Construye
 * un arreglo ORDENADO donde arr[k] = k (así el índice correcto de cualquier
 * búsqueda es el propio valor buscado, sin ambigüedad), corre el kernel, y
 * antes de imprimir el tiempo VERIFICA que el resultado devuelto sea
 * EXACTAMENTE `SEARCHES` (todas las búsquedas encontraron su índice
 * correcto) — si no, imprime `-1` en vez de un tiempo que alimentaría el
 * ajuste de exponente con un kernel roto. Escrito por Sythrall, nunca código
 * de usuario.
 *
 * `SEARCHES` es fijo (no varía con `n`, a propósito — ver el comentario de
 * `validate_binary_search.s` para el porqué) así que vive acá como
 * constante de compilación, no como argv adicional. */
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define SEARCHES 4000000

extern int binary_search_batch(int *arr, int n, int searches);

int main(int argc, char **argv) {
    if (argc < 2) return 1;
    long n = atol(argv[1]);
    int *arr = malloc(sizeof(int) * (size_t)n);
    if (!arr) return 1;

    for (long i = 0; i < n; i++) {
        arr[i] = (int)i;
    }

    clock_t start = clock();
    int correct = binary_search_batch(arr, (int)n, SEARCHES);
    clock_t end = clock();

    if (correct != SEARCHES) {
        fprintf(stderr, "BINARY SEARCH BROKEN: %d/%d correctas\n", correct, SEARCHES);
        printf("-1\n");
        free(arr);
        return 0;
    }

    double seconds = (double)(end - start) / (double)CLOCKS_PER_SEC;
    printf("%.9f\n", seconds);

    free(arr);
    return 0;
}
