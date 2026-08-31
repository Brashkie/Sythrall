# Fase 26 (Algorithm Validation Engine) — sexto kernel, y el primero que
# demuestra la forma O(n log n): mergesort bottom-up ITERATIVO (sin
# recursión — mucho más simple de escribir correctamente a mano que un
# quicksort recursivo, y O(n log n) por construcción: log2(n) pasadas de
# fusión, cada una O(n)). Escrito por Sythrall, NUNCA código de usuario —
# mismo límite de seguridad que `validate_sum_squares.s`. Convención cdecl
# de 32 bits (mismo toolchain MinGW de esta máquina, confirmado con
# `gcc -dumpmachine` → mingw32).
#
# void mergesort_bu(int *arr, int *temp, int n) — cada pasada dobla el
# tamaño del bloque fusionado (width), empezando en 1: fusiona arr[i..i+w)
# con arr[i+w..i+2w) hacia `temp`, después copia el resultado de vuelta a
# `arr`. `temp` es un buffer auxiliar del mismo tamaño que `arr` — el
# driver C lo reserva porque un objeto `.s` no puede hacer malloc por sí
# solo.
#
# BUG REAL atrapado por un debugging exhaustivo con guard pages
# (VirtualAlloc + VirtualProtect PAGE_NOACCESS), no adivinado: la primera
# versión de este kernel usaba offsets de locales -4(%ebp) a -32(%ebp),
# ASUMIENDO que esas posiciones eran locales propios — pero como
# `push %ebx`/`push %esi`/`push %edi` ocurren DESPUÉS de `mov %esp,%ebp`,
# esas 3 posiciones (-4,-8,-12) son en realidad donde quedan GUARDADOS
# ebx/esi/edi, no espacio libre. Escribir ahí corrompía los valores
# guardados, así que el `pop` del epílogo restauraba basura en esos
# registros al retornar. El bug era invisible a -O0 (el caller recarga
# todo desde su propio stack, nunca confía en que ebx/esi/edi sobrevivan a
# la llamada) pero causaba un crash real a -O1 y superior, donde GCC SÍ
# cachea valores como el puntero `arr` en un registro callee-saved a través
# de la llamada, confiando en que la función invocada lo preserve — exactamente
# lo que el ABI cdecl exige y lo que este bug rompía. Fix: los locales
# empiezan en -16(%ebp), dejando -4/-8/-12 para los registros guardados.
# Verificado con guard pages para n = 0..1,000,000 en -O0/-O1/-O2/-O3 antes
# de dar el bug por resuelto.
    .text
    .globl _mergesort_bu
    .globl mergesort_bu
# Dos labels para el mismo código: Windows/MinGW (32 bits) antepone `_` a
# los símbolos C (por eso `_mergesort_bu`), pero un `gcc`/`ld` de Linux
# (ELF, sin ese prefijo) busca `mergesort_bu` a secas — sin el segundo
# label, el link falla con "undefined reference to mergesort_bu" en
# cualquier entorno que no sea Windows (bug real atrapado en CI, mismo
# problema que `validate_sum_squares.s`, ver su comentario para el detalle).
_mergesort_bu:
mergesort_bu:
    push %ebp
    mov %esp, %ebp
    push %ebx
    push %esi
    push %edi
    sub $32, %esp
    # ebp-4/-8/-12 son los registros callee-saved guardados (ebx/esi/edi),
    # NO locales propios -- los locales empiezan en -16.
    # -16(%ebp) width
    # -20(%ebp) i
    # -24(%ebp) left
    # -28(%ebp) mid
    # -32(%ebp) right
    # -36(%ebp) ii
    # -40(%ebp) jj
    # -44(%ebp) kk

    movl $1, -16(%ebp)
.Louter_width_loop:
    movl -16(%ebp), %eax
    cmpl 16(%ebp), %eax
    jge .Ldone
    movl $0, -20(%ebp)
.Louter_i_loop:
    movl -20(%ebp), %eax
    cmpl 16(%ebp), %eax
    jge .Lnext_width

    movl -20(%ebp), %eax
    movl %eax, -24(%ebp)

    movl -20(%ebp), %eax
    addl -16(%ebp), %eax
    cmpl 16(%ebp), %eax
    jle .Lmid_ok
    movl 16(%ebp), %eax
.Lmid_ok:
    movl %eax, -28(%ebp)

    movl -20(%ebp), %eax
    movl -16(%ebp), %edx
    addl %edx, %edx
    addl %edx, %eax
    cmpl 16(%ebp), %eax
    jle .Lright_ok
    movl 16(%ebp), %eax
.Lright_ok:
    movl %eax, -32(%ebp)

    movl -24(%ebp), %eax
    movl %eax, -36(%ebp)
    movl -28(%ebp), %eax
    movl %eax, -40(%ebp)
    movl -24(%ebp), %eax
    movl %eax, -44(%ebp)

.Lmerge_main_loop:
    movl -36(%ebp), %eax
    cmpl -28(%ebp), %eax
    jge .Lmerge_left_done
    movl -40(%ebp), %eax
    cmpl -32(%ebp), %eax
    jge .Lmerge_left_done

    movl 8(%ebp), %esi
    movl -36(%ebp), %eax
    movl (%esi,%eax,4), %ecx
    movl -40(%ebp), %edx
    movl (%esi,%edx,4), %ebx
    cmpl %ebx, %ecx
    jg .Ltake_right

.Ltake_left:
    movl 12(%ebp), %edi
    movl -44(%ebp), %eax
    movl %ecx, (%edi,%eax,4)
    incl -36(%ebp)
    incl -44(%ebp)
    jmp .Lmerge_main_loop

.Ltake_right:
    movl 12(%ebp), %edi
    movl -44(%ebp), %eax
    movl %ebx, (%edi,%eax,4)
    incl -40(%ebp)
    incl -44(%ebp)
    jmp .Lmerge_main_loop

.Lmerge_left_done:
.Lmerge_copy_left_loop:
    movl -36(%ebp), %eax
    cmpl -28(%ebp), %eax
    jge .Lmerge_copy_right_loop
    movl 8(%ebp), %esi
    movl -36(%ebp), %eax
    movl (%esi,%eax,4), %ecx
    movl 12(%ebp), %edi
    movl -44(%ebp), %eax
    movl %ecx, (%edi,%eax,4)
    incl -36(%ebp)
    incl -44(%ebp)
    jmp .Lmerge_copy_left_loop

.Lmerge_copy_right_loop:
    movl -40(%ebp), %eax
    cmpl -32(%ebp), %eax
    jge .Lmerge_writeback
    movl 8(%ebp), %esi
    movl -40(%ebp), %eax
    movl (%esi,%eax,4), %ecx
    movl 12(%ebp), %edi
    movl -44(%ebp), %eax
    movl %ecx, (%edi,%eax,4)
    incl -40(%ebp)
    incl -44(%ebp)
    jmp .Lmerge_copy_right_loop

.Lmerge_writeback:
    movl -24(%ebp), %eax
    movl %eax, -36(%ebp)
.Lmerge_writeback_loop:
    movl -36(%ebp), %eax
    cmpl -32(%ebp), %eax
    jge .Lmerge_done
    movl 12(%ebp), %esi
    movl -36(%ebp), %eax
    movl (%esi,%eax,4), %ecx
    movl 8(%ebp), %edi
    movl -36(%ebp), %eax
    movl %ecx, (%edi,%eax,4)
    incl -36(%ebp)
    jmp .Lmerge_writeback_loop

.Lmerge_done:
    movl -16(%ebp), %eax
    addl %eax, %eax
    addl %eax, -20(%ebp)
    jmp .Louter_i_loop

.Lnext_width:
    movl -16(%ebp), %eax
    addl %eax, %eax
    movl %eax, -16(%ebp)
    jmp .Louter_width_loop

.Ldone:
    add $32, %esp
    pop %edi
    pop %esi
    pop %ebx
    pop %ebp
    ret
