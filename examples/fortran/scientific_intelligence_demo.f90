! Sythrall — demo de Fase 20 (Scientific Intelligence)
!
! Subilo en el panel "Static Analysis" (o Upload) para ver las 3 señales
! nuevas de Fortran en acción, una por subrutina:
!
!   SCALE_VECTOR -> badge "SIMD?"          (candidato a vectorización)
!   MATMULT      -> badge "Matrix?"        (forma de multiplicación de matrices, O(n³))
!   WRAP_DGEMM   -> badge "BLAS/LAPACK"    (uso de una rutina BLAS real)

SUBROUTINE SCALE_VECTOR(A, N)
  REAL :: A(N)
  INTEGER :: N, I
  DO I = 1, N
    A(I) = A(I) * 2.0
  END DO
END SUBROUTINE SCALE_VECTOR


SUBROUTINE MATMULT(A, B, C, N)
  REAL :: A(N,N), B(N,N), C(N,N)
  INTEGER :: N, I, J, K
  DO I = 1, N
    DO J = 1, N
      DO K = 1, N
        C(I,J) = C(I,J) + A(I,K) * B(K,J)
      END DO
    END DO
  END DO
END SUBROUTINE MATMULT


SUBROUTINE WRAP_DGEMM(A, B, C, N)
  REAL :: A(N,N), B(N,N), C(N,N)
  INTEGER :: N
  CALL DGEMM('N', 'N', N, N, N, 1.0, A, N, B, N, 0.0, C, N)
END SUBROUTINE WRAP_DGEMM
