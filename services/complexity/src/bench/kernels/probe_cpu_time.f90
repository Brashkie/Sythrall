! Mide el tick real del reloj de `CPU_TIME` en ESTA máquina.
!
! El piso de resolución que `descartar_bajo_resolucion` aplica estaba
! hardcodeado al peor caso observado —1/64 s, que es lo que avanza `CPU_TIME`
! de gfortran en Windows—. Tratar el peor caso como universal no es
! conservador: en una máquina con un reloj fino descarta mediciones buenas y
! el validador contesta "no se pudo medir" teniendo datos de sobra. Eso es
! exactamente lo que pasó en CI (ubuntu-latest) con los tamaños 450 y 600.
!
! Usa `REAL` de precisión simple y `CPU_TIME`, igual que `validate_matmul.f90`:
! si midiera con otro reloj u otra precisión, el número no aplicaría al kernel
! que tiene que filtrar.
!
! Espera activa hasta que el reloj avance una vez y reporta ese salto, que es
! el incremento más chico observable en la práctica. Dura un tick como mucho.
PROGRAM SYTHRALL_PROBE_CPU_TIME
  IMPLICIT NONE
  REAL :: T0, T1
  CALL CPU_TIME(T0)
  T1 = T0
  DO WHILE (T1 == T0)
    CALL CPU_TIME(T1)
  END DO
  PRINT *, T1 - T0
END PROGRAM SYTHRALL_PROBE_CPU_TIME
