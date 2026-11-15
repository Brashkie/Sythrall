// ══════════════════════════════════════════
//  Sythrall — Diálogos de la app (reemplazo de prompt/confirm nativos)
// ══════════════════════════════════════════
// utils/dialog.ts
//
// La app usaba `window.prompt`/`window.confirm` en 5 lugares (crear proyecto
// al soltar archivos, crear archivo, borrar proyecto ×2, limpiar todo).
// Problemas reales, no de gusto:
//   - **Bloquean el hilo principal**: mientras el modal nativo está abierto,
//     la app entera se congela. Un `prompt` disparado en medio de un
//     drag&drop deja la animación de la dropzone trabada.
//   - **Ignoran el tema**: son chrome del navegador, así que en tema claro u
//     oscuro se ven igual — y ajenos a todo el resto de la UI.
//   - **En móvil** son un modal del sistema operativo, con su propio
//     teclado y su propio estilo.
//   - Algunos navegadores los suprimen del todo (iframes, múltiples
//     diálogos seguidos), y ahí la acción simplemente no pasa, sin aviso.
//
// Estos diálogos usan los mismos tokens que el resto de la app, devuelven una
// Promise (así el caller sigue leyéndose igual que con `await`), y cubren lo
// que el nativo daba gratis y suele olvidarse al reemplazarlo: foco inicial
// en el control correcto, Escape para cancelar, Enter para confirmar, clic en
// el fondo para cerrar, `role="dialog"`/`aria-modal` para lectores de
// pantalla, y devolución del foco al elemento que lo abrió.

import { esc } from './helpers'

interface DialogOptions {
  title: string
  message?: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
}

interface TextDialogOptions extends DialogOptions {
  placeholder?: string
  initialValue?: string
}

type Resolver<T> = (value: T) => void

function buildOverlay(inner: string): HTMLElement {
  const overlay = document.createElement('div')
  overlay.className = 'app-dialog-overlay open'
  overlay.innerHTML = `<div class="app-dialog" role="dialog" aria-modal="true" aria-labelledby="app-dialog-title">${inner}</div>`
  document.body.appendChild(overlay)
  return overlay
}

function headHtml(title: string, message?: string): string {
  return `
    <div class="app-dialog-title" id="app-dialog-title">${esc(title)}</div>
    ${message ? `<div class="app-dialog-msg">${esc(message)}</div>` : ''}`
}

function footHtml(o: DialogOptions): string {
  return `
    <div class="app-dialog-actions">
      <button type="button" class="btn btn-ghost btn-sm" data-dlg="cancel">${esc(o.cancelLabel ?? 'Cancelar')}</button>
      <button type="button" class="btn ${o.danger ? 'btn-danger' : 'btn-run'} btn-sm" data-dlg="ok">${esc(o.confirmLabel ?? 'Aceptar')}</button>
    </div>`
}

/** Cablea cierre por Escape / clic en el fondo / botones, y restaura el foco.
 * `finish` se llama una sola vez pase lo que pase — sin esto, un Escape
 * después de un clic dejaría la Promise resuelta dos veces. */
function wire<T>(overlay: HTMLElement, resolve: Resolver<T>, read: (ok: boolean) => T): void {
  const previouslyFocused = document.activeElement as HTMLElement | null
  let done = false
  const finish = (ok: boolean) => {
    if (done) return
    done = true
    document.removeEventListener('keydown', onKey, true)
    overlay.remove()
    previouslyFocused?.focus?.()
    resolve(read(ok))
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      finish(false)
    } else if (e.key === 'Enter' && !(e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault()
      finish(true)
    }
  }
  document.addEventListener('keydown', onKey, true)
  overlay.addEventListener('click', (e) => {
    const t = e.target as HTMLElement
    if (t === overlay) return finish(false)
    const action = t.closest<HTMLElement>('[data-dlg]')?.dataset['dlg']
    if (action) finish(action === 'ok')
  })
}

/** Reemplazo de `window.prompt`. `null` = el usuario canceló (misma
 * convención que el nativo, así los callers no cambian su lógica). */
export function askText(o: TextDialogOptions): Promise<string | null> {
  return new Promise((resolve) => {
    const overlay = buildOverlay(`
      ${headHtml(o.title, o.message)}
      <input type="text" class="app-dialog-input" value="${esc(o.initialValue ?? '')}"
             placeholder="${esc(o.placeholder ?? '')}" />
      ${footHtml(o)}`)
    const input = overlay.querySelector<HTMLInputElement>('.app-dialog-input')!
    wire(overlay, resolve, (ok) => (ok ? input.value.trim() : null))
    input.focus()
    input.select()
  })
}

/** Una opción del diálogo de selección. `disabled` NO se usa para "no te
 * dejo": se usa para mostrar lo que existe pero hoy no aplica, con el motivo
 * al lado. Esconder la opción dejaría al usuario preguntándose por qué no
 * está; mostrarla apagada con "sin endpoints cargados" responde eso solo. */
export interface ChoiceOption {
  id: string
  label: string
  desc?: string
  checked: boolean
  disabled?: boolean
  disabledReason?: string
}

/** Selección múltiple. Devuelve los ids elegidos, o `null` si se canceló —
 * misma convención que `askText`, así el caller distingue "cancelé" de
 * "confirmé sin elegir nada". */
export function askChoices(o: DialogOptions & { options: ChoiceOption[] }): Promise<string[] | null> {
  return new Promise((resolve) => {
    const filas = o.options
      .map(
        (op) => `
      <label class="app-dialog-choice${op.disabled ? ' is-disabled' : ''}">
        <input type="checkbox" value="${esc(op.id)}" ${op.checked && !op.disabled ? 'checked' : ''} ${op.disabled ? 'disabled' : ''} />
        <span class="app-dialog-choice-body">
          <span class="app-dialog-choice-label">${esc(op.label)}</span>
          ${op.disabled && op.disabledReason ? `<span class="app-dialog-choice-why">${esc(op.disabledReason)}</span>` : op.desc ? `<span class="app-dialog-choice-desc">${esc(op.desc)}</span>` : ''}
        </span>
      </label>`,
      )
      .join('')
    const overlay = buildOverlay(`
      ${headHtml(o.title, o.message)}
      <div class="app-dialog-choices">${filas}</div>
      ${footHtml(o)}`)
    wire(overlay, resolve, (ok) =>
      ok
        ? Array.from(overlay.querySelectorAll<HTMLInputElement>('input[type="checkbox"]:checked')).map((i) => i.value)
        : null,
    )
    overlay.querySelector<HTMLElement>('[data-dlg="ok"]')?.focus()
  })
}

/** Reemplazo de `window.confirm`. */
export function askConfirm(o: DialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = buildOverlay(`${headHtml(o.title, o.message)}${footHtml(o)}`)
    wire(overlay, resolve, (ok) => ok)
    overlay.querySelector<HTMLElement>('[data-dlg="ok"]')?.focus()
  })
}
