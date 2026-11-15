! Fase 26 (Algorithm Validation Engine) — kernel de validación empírica.
! Escrito por Sythrall, NUNCA código de usuario ni de un archivo subido —
! mismo límite de seguridad que documenta fortran_bench.rs/linreg_bench.rs.
! Esta es la pieza 3 de "migrar de numpy/pandas/scikit-learn" (pedido
! explícito del usuario, 2026-08-31): antes de proponer reemplazar
! sklearn.LinearRegression por un kernel nativo, medir HONESTAMENTE contra
! sklearn real — ver routers/execution.py::validate_linreg_vs_sklearn.
!
! Regresión lineal univariada por mínimos cuadrados, forma cerrada (ecuación
! normal de 2 variables) — O(n), un solo recorrido de los datos. Deliberadamente
! NO una versión multivariada: eso necesita álgebra lineal real (inversión de
! matriz / eliminación gaussiana), fuera de alcance para esta primera pieza.
PROGRAM SYTHRALL_VALIDATE_LINREG
  IMPLICIT NONE
  INTEGER :: N, I
  REAL, ALLOCATABLE :: X(:), Y(:)
  REAL :: T1, T2
  REAL :: SUM_X, SUM_Y, SUM_XY, SUM_X2, SLOPE, INTERCEPT
  CHARACTER(LEN=32) :: ARG
  CALL GET_COMMAND_ARGUMENT(1, ARG)
  READ(ARG, *) N
  ALLOCATE(X(N), Y(N))
  ! Datos deterministas: y = 3x + 7 + ruido entero chico — mismos valores que
  ! el lado Python genera para comparar contra sklearn real (misma fórmula,
  ! mismo criterio que validate_matmul.f90/kernels de agregación).
  DO I = 1, N
    X(I) = REAL(I)
    Y(I) = 3.0*REAL(I) + 7.0 + REAL(MOD(I*37+11, 21) - 10)
  END DO
  CALL CPU_TIME(T1)
  SUM_X = 0.0
  SUM_Y = 0.0
  SUM_XY = 0.0
  SUM_X2 = 0.0
  DO I = 1, N
    SUM_X = SUM_X + X(I)
    SUM_Y = SUM_Y + Y(I)
    SUM_XY = SUM_XY + X(I)*Y(I)
    SUM_X2 = SUM_X2 + X(I)*X(I)
  END DO
  SLOPE = (REAL(N)*SUM_XY - SUM_X*SUM_Y) / (REAL(N)*SUM_X2 - SUM_X*SUM_X)
  INTERCEPT = (SUM_Y - SLOPE*SUM_X) / REAL(N)
  CALL CPU_TIME(T2)
  ! SLOPE/INTERCEPT nunca se leen después de esto — sin imprimirlos, gfortran
  ! -O2 detecta que todo el recorrido de arriba no tiene ningún efecto
  ! observable y elimina el loop entero (confirmado en vivo: sin esta línea,
  ! CADA tamaño de N, incluso 200 millones, medía 0.0 segundos). Segunda
  ! línea de salida, ignorada por el lado Rust (que solo parsea la primera) —
  ! existe únicamente para que el optimizador no pueda borrar el cómputo.
  PRINT *, T2 - T1
  PRINT *, SLOPE, INTERCEPT
END PROGRAM SYTHRALL_VALIDATE_LINREG
