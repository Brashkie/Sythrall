/* Fase 26 (Algorithm Validation Engine) — driver C mínimo para el kernel de
 * Assembly (`validate_sum_squares.s`). Necesario porque un objeto .s puro
 * no trae su propio entry point/runtime — este driver solo parsea argv[1],
 * mide el tiempo de la llamada real a la función en Assembly, e imprime el
 * resultado en el mismo formato que el resto de los kernels de validación
 * (segundos en stdout). Escrito por Sythrall, nunca código de usuario. */
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

extern long sum_squares(long n);

int main(int argc, char **argv) {
    if (argc < 2) return 1;
    long n = atol(argv[1]);
    clock_t start = clock();
    long result = sum_squares(n);
    clock_t end = clock();
    double elapsed = (double)(end - start) / (double)CLOCKS_PER_SEC;
    fprintf(stderr, "result=%ld\n", result);
    printf("%f\n", elapsed);
    return 0;
}
