/* Fase 26 (Algorithm Validation Engine) — driver C mínimo para el kernel de
 * Assembly (`validate_mergesort.s`), mismo motivo que
 * `validate_sum_squares_driver.c`: un objeto `.s` puro no trae su propio
 * entry point ni puede reservar memoria (`temp` es un buffer del mismo
 * tamaño que `arr`, necesario para la fusión). Genera datos deterministas
 * (LCG simple, mismos datos en cada corrida) y, antes de imprimir el
 * tiempo, VERIFICA que el resultado realmente quedó ordenado — si no,
 * imprime `-1` en vez de un tiempo que pasaría el ajuste de exponente en
 * silencio con un kernel roto. Escrito por Sythrall, nunca código de
 * usuario. */
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

extern void mergesort_bu(int *arr, int *temp, int n);

int main(int argc, char **argv) {
    if (argc < 2) return 1;
    long n = atol(argv[1]);
    int *arr = malloc(sizeof(int) * (size_t)n);
    int *temp = malloc(sizeof(int) * (size_t)n);
    if (!arr || !temp) return 1;

    unsigned int seed = 12345u;
    for (long i = 0; i < n; i++) {
        seed = seed * 1103515245u + 12345u;
        arr[i] = (int)((seed >> 16) % 1000000u);
    }

    clock_t start = clock();
    mergesort_bu(arr, temp, (int)n);
    clock_t end = clock();

    for (long i = 1; i < n; i++) {
        if (arr[i - 1] > arr[i]) {
            fprintf(stderr, "SORT BROKEN at index %ld: %d > %d\n", i, arr[i - 1], arr[i]);
            printf("-1\n");
            free(arr);
            free(temp);
            return 0;
        }
    }

    double seconds = (double)(end - start) / (double)CLOCKS_PER_SEC;
    printf("%.9f\n", seconds);

    free(arr);
    free(temp);
    return 0;
}
