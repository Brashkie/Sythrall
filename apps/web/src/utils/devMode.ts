// ══════════════════════════════════════════
//  Sythrall — Modo desarrollador
// ══════════════════════════════════════════
// utils/devMode.ts
//
// Fase 33 (Product Shell), bullet 10 de la tabla de prioridades del usuario:
// el hogar definitivo de puertos, `/health` y debug.
//
// **Qué gatea y qué no, que es la decisión de fondo.** No todo lo técnico es
// "de desarrollo". El probador de endpoints (Servicios → APIs) es una
// funcionalidad del producto: alguien que monitorea sus propias APIs lo usa
// para eso, no para depurar Sythrall. Lo que sí es andamio es todo lo que
// habla de la INFRAESTRUCTURA DE SYTHRALL MISMO — los atajos a `:8420`,
// `:3000` y `:5173`, y el detalle de qué corre dónde. Eso vive acá.
//
// **Apagado por defecto, a propósito.** El usuario que abre Sythrall no tiene
// por qué enterarse de que hay tres procesos; el que sí necesita saberlo lo
// prende una vez y se queda prendido.
//
// **Cómo se aplica.** Un atributo en `<html>` en vez de una clase por
// elemento: así el CSS decide qué se ve sin que ningún componente tenga que
// consultar el estado ni volver a renderizarse. Mismo enfoque que el tema.

const DEV_MODE_KEY = 'sythrall:dev-mode'

/** Evento emitido en cada cambio, para que el modal de Configuración
 * repinte su sección sin acoplarse a este módulo al revés. */
const DEV_MODE_EVENT = 'sythrall:dev-mode'

export function isDevMode(): boolean {
  try {
    return localStorage.getItem(DEV_MODE_KEY) === 'on'
  } catch {
    // Modo privado / storage bloqueado: apagado, que es el default seguro.
    return false
  }
}

export function applyDevMode(): void {
  const on = isDevMode()
  if (on) document.documentElement.dataset['devmode'] = 'on'
  else delete document.documentElement.dataset['devmode']
}

export function setDevMode(on: boolean): void {
  try {
    localStorage.setItem(DEV_MODE_KEY, on ? 'on' : 'off')
  } catch {
    /* igual se aplica en esta sesión */
  }
  applyDevMode()
  document.dispatchEvent(new CustomEvent(DEV_MODE_EVENT, { detail: on }))
}
