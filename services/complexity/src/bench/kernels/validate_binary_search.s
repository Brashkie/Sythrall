# Fase 26 (Algorithm Validation Engine) — décimo kernel, y el primero en
# validar una forma SUB-LINEAL: búsqueda binaria iterativa escrita a mano en
# Assembly x86 (tercer kernel en Assembly, junto a `validate_sum_squares.s`
# O(n) y `validate_mergesort.s` O(n log n)). Escrito por Sythrall, NUNCA
# código de usuario — mismo límite de seguridad que el resto de Fase 26.
# Convención cdecl de 32 bits (mismo toolchain MinGW de esta máquina,
# confirmado con `gcc -dumpmachine` -> mingw32).
#
# int binary_search_batch(int *arr, int n, int searches) — `arr` es un
# arreglo ORDENADO de `n` enteros donde arr[k] = k (construido así por el
# driver C), así que el índice correcto para buscar el valor T es
# exactamente T. Hace `searches` búsquedas con targets PSEUDO-ALEATORIOS
# (mismo LCG que `validate_mergesort_driver.c` ya usa para generar datos:
# `state = state*1103515245 + 12345`) en vez de un target secuencial, y
# cuenta cuántas encontraron el índice correcto. El caller (Rust, vía el
# driver) valida que el resultado devuelto sea EXACTAMENTE `searches` —
# cualquier valor menor es un kernel roto, no un tiempo válido para
# alimentar el ajuste de exponente.
#
# **Por qué pseudo-aleatorio y no `i mod n` (medido en vivo, no adivinado)**:
# la primera versión ciclaba el target secuencialmente (0,1,2,...,n-1,0,...).
# Eso mide algo real, pero un patrón de accesos tan regular deja que el
# predictor de saltos de la CPU aprenda la secuencia de izquierda/derecha de
# cada búsqueda, subestimando el costo real de una búsqueda binaria contra
# targets arbitrarios. Un LCG con AND-mask (`target = (state >> 16) & (n-1)`
# — requiere que `n` sea potencia de 2, ya lo es por construcción, ver
# `KERNEL_SIZES` en `binsearch_bench.rs`) da un patrón no correlacionado sin
# pagar el costo de un `idiv` (`i mod n` sí lo pagaría, y ese costo es FIJO
# por búsqueda, no escala con `log2(n)` — diluye exactamente la señal que
# este kernel existe para medir).
#
# Por qué "searches" fijo en vez de UNA sola búsqueda por tamaño: una
# búsqueda binaria individual hace ~log2(n) comparaciones -- para el rango
# de `n` que este kernel usa (miles a decenas de millones), eso son apenas
# 12 a 24 comparaciones, muy por debajo de la resolución de `clock()` en
# Windows (~15.6ms) para dar un tiempo medible. Mismo problema ya resuelto
# para `validate_sum_squares.s` (cientos de millones de iteraciones) --
# acá la solución es repetir la búsqueda `searches` veces (fijo, igual para
# todos los tamaños), así el tiempo total escala con `searches * log2(n)`,
# medible de sobra.
#
# `-16(%ebp)` en adelante para los locales -- NO `-4(%ebp)`, la misma
# lección real de `validate_mergesort.s`: `push %ebx` ocurre DESPUÉS de
# fijar %ebp, así que -4(%ebp) es donde queda ese registro guardado, no
# espacio libre para locales propios (a diferencia de `validate_mergesort.s`,
# este kernel no usa `%esi`/`%edi` -- todo el trabajo cabe en
# `%eax`/`%ecx`/`%edx`/`%ebx` -- así que solo se guarda `%ebx`, no los 3).
    .text
    .globl _binary_search_batch
    .globl binary_search_batch
# Dos labels para el mismo código: Windows/MinGW (32 bits) antepone `_` a
# los símbolos C, pero un `gcc`/`ld` de Linux (ELF) busca el nombre sin
# prefijo -- mismo problema real de portabilidad ya documentado en
# `validate_sum_squares.s`/`validate_mergesort.s`.
_binary_search_batch:
binary_search_batch:
    push %ebp
    mov %esp, %ebp
    push %ebx
    sub $32, %esp
    # -16(%ebp) correct_count
    # -20(%ebp) i (contador externo: 0..searches)
    # -24(%ebp) target
    # -28(%ebp) lo
    # -32(%ebp) hi
    # -36(%ebp) lcg_state
    # Args: arr=8(%ebp) n=12(%ebp) searches=16(%ebp)

    movl $0, -16(%ebp)
    movl $0, -20(%ebp)
    movl $12345, -36(%ebp)

.Louter_loop:
    movl -20(%ebp), %eax
    cmpl 16(%ebp), %eax
    jge .Louter_done

    # target = (lcg_state >> 16) & (n - 1) -- ver el comentario del módulo
    # arriba para el porqué (pseudo-aleatorio, sin idiv).
    movl -36(%ebp), %eax
    imull $1103515245, %eax, %eax
    addl $12345, %eax
    movl %eax, -36(%ebp)
    shrl $16, %eax
    movl 12(%ebp), %ecx
    decl %ecx
    andl %ecx, %eax
    movl %eax, -24(%ebp)

    movl $0, -28(%ebp)
    movl 12(%ebp), %eax
    decl %eax
    movl %eax, -32(%ebp)

.Lsearch_loop:
    movl -28(%ebp), %eax
    cmpl -32(%ebp), %eax
    jg .Lsearch_not_found

    # mid = lo + (hi - lo) / 2 -- nunca `(lo + hi) / 2` (ese overflow no es
    # un riesgo real acá dado el rango de `n` de este kernel, pero la forma
    # `lo + (hi-lo)/2` es la correcta en general y no cuesta nada usarla).
    movl -32(%ebp), %eax
    subl -28(%ebp), %eax
    sarl $1, %eax
    addl -28(%ebp), %eax
    movl %eax, %ecx

    movl 8(%ebp), %ebx
    movl (%ebx,%ecx,4), %edx

    cmpl -24(%ebp), %edx
    je .Lfound
    jl .Lgo_right

.Lgo_left:
    decl %ecx
    movl %ecx, -32(%ebp)
    jmp .Lsearch_loop

.Lgo_right:
    incl %ecx
    movl %ecx, -28(%ebp)
    jmp .Lsearch_loop

.Lfound:
    # arr[k] = k por construcción -- el índice encontrado (ecx = mid) debe
    # ser EXACTAMENTE el target. Si no lo es, algo del lado del driver (o de
    # esta misma función) está roto: no se cuenta como acierto, así que el
    # chequeo `correct_count == searches` del driver lo atrapa en vez de
    # reportar un tiempo con un kernel silenciosamente incorrecto.
    cmpl -24(%ebp), %ecx
    jne .Lsearch_done
    incl -16(%ebp)
    jmp .Lsearch_done

.Lsearch_not_found:
.Lsearch_done:
    incl -20(%ebp)
    jmp .Louter_loop

.Louter_done:
    movl -16(%ebp), %eax
    add $32, %esp
    pop %ebx
    pop %ebp
    ret
