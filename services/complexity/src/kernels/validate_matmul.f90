! Fase 23 (Execution Intelligence) — kernel de validación empírica de Big-O.
! Escrito por Sythrall, NUNCA código de usuario ni de un archivo subido —
! ver el doc de módulo de `fortran_bench.rs` para el límite de seguridad
! exacto. Compilado y ejecutado por ese mismo módulo (`include_str!`, sin
! volver a parsear ni transformar este archivo) a varios tamaños de N para
! medir si una multiplicación de matrices N×N escala como O(n³) de verdad,
! no solo por su forma estática.
PROGRAM SYTHRALL_VALIDATE_MATMUL
  IMPLICIT NONE
  INTEGER :: N, I, J, K
  REAL, ALLOCATABLE :: A(:,:), B(:,:), C(:,:)
  REAL :: T1, T2
  CHARACTER(LEN=32) :: ARG
  CALL GET_COMMAND_ARGUMENT(1, ARG)
  READ(ARG, *) N
  ALLOCATE(A(N,N), B(N,N), C(N,N))
  DO I = 1, N
    DO J = 1, N
      A(I,J) = REAL(MOD(I+J,7))
      B(I,J) = REAL(MOD(I*J,5))
      C(I,J) = 0.0
    END DO
  END DO
  CALL CPU_TIME(T1)
  DO I = 1, N
    DO J = 1, N
      DO K = 1, N
        C(I,J) = C(I,J) + A(I,K) * B(K,J)
      END DO
    END DO
  END DO
  CALL CPU_TIME(T2)
  PRINT *, T2 - T1
END PROGRAM SYTHRALL_VALIDATE_MATMUL
