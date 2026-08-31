# Fase 26 (Algorithm Validation Engine) — kernel de validación empírica en
# Assembly x86 real (GNU as, sintaxis AT&T — la misma que `asmparse.rs` ya
# sabe leer), generalizando fortran_bench.rs más allá de Fortran/matmul.
# Escrito por Sythrall, NUNCA código de usuario — mismo límite de seguridad
# que fortran_bench.rs. Convención cdecl de 32 bits (el toolchain MinGW de
# esta máquina de desarrollo es de 32 bits): el único argumento (`n`) llega
# en la pila, en 8(%ebp); el resultado vuelve en %eax.
#
# long sum_squares(long n) — suma 1² + 2² + ... + n². Se desborda en 32 bits
# para N grande (deliberado y aceptado: este kernel valida TIEMPO de
# ejecución, no el valor devuelto — la correctitud del valor se verificó
# aparte con N chico, ver `asm_bench.rs::tests`).
    .text
    .globl _sum_squares
_sum_squares:
    push %ebp
    mov %esp, %ebp
    push %ebx            # ebx es callee-saved en cdecl, hay que preservarlo
    mov 8(%ebp), %ecx     # n
    xor %eax, %eax         # eax = suma = 0
    xor %edx, %edx          # edx = i = 0
    inc %edx
.Lloop:
    cmp %ecx, %edx
    jg .Ldone
    mov %edx, %ebx
    imul %ebx, %ebx         # ebx = i*i
    add %ebx, %eax           # suma += i*i
    inc %edx
    jmp .Lloop
.Ldone:
    pop %ebx
    pop %ebp
    ret
