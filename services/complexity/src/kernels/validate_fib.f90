! Fase 26 (Algorithm Validation Engine) — kernel de validación de
! profundidad de recursión: Fibonacci recursivo ingenuo, SIN memoización a
! propósito (memoizar lo volvería O(n), que no es lo que este kernel valida).
! Escrito por Sythrall, NUNCA código de usuario — ver el doc de módulo de
! `fib_bench.rs` para el límite de seguridad exacto. Compilado y ejecutado
! por ese mismo módulo (`include_str!`) a varios tamaños de N para medir si
! la recursión doble realmente crece exponencialmente, y con qué base.
RECURSIVE FUNCTION FIB(N) RESULT(R)
  IMPLICIT NONE
  INTEGER, INTENT(IN) :: N
  INTEGER :: R
  IF (N <= 1) THEN
    R = N
  ELSE
    R = FIB(N-1) + FIB(N-2)
  END IF
END FUNCTION FIB

PROGRAM SYTHRALL_VALIDATE_FIB
  IMPLICIT NONE
  INTEGER :: N, R
  REAL :: T1, T2
  CHARACTER(LEN=32) :: ARG
  INTERFACE
    RECURSIVE FUNCTION FIB(N) RESULT(R)
      INTEGER, INTENT(IN) :: N
      INTEGER :: R
    END FUNCTION FIB
  END INTERFACE
  CALL GET_COMMAND_ARGUMENT(1, ARG)
  READ(ARG, *) N
  CALL CPU_TIME(T1)
  R = FIB(N)
  CALL CPU_TIME(T2)
  PRINT *, T2 - T1
END PROGRAM SYTHRALL_VALIDATE_FIB
