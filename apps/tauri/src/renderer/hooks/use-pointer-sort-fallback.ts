import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from 'react'

export type PointerSortTarget = {
  id: string
  kind: string
  element: HTMLElement
}

type PointerSortState<T> = {
  source: T
  sourceElement: HTMLElement
  pressX: number
  pressY: number
  originLeft: number
  originTop: number
  dragStartX: number
  dragStartY: number
  pointerId: number
  active: boolean
} | null

const TARGET_SELECTOR = '[data-fileterm-sort-id]'
const INTERACTIVE_SELECTOR = 'button, input, textarea, select, a, [contenteditable="true"]'

/**
 * Tauri's macOS WebView does not consistently complete HTML5 drag/drop for
 * in-page sortable rows. This uses pointer movement as the authoritative
 * local-sort path while leaving native drag/drop available for external files.
 */
export function usePointerSortFallback<T>({
  onStart,
  onTarget,
  onDrop,
  onCancel
}: {
  onStart(source: T): void
  onTarget(source: T, target: PointerSortTarget, clientY: number): void
  onDrop(source: T, target: PointerSortTarget | null, clientY: number): void
  onCancel(): void
}) {
  const stateRef = useRef<PointerSortState<T>>(null)
  const ghostRef = useRef<HTMLElement | null>(null)
  const callbacksRef = useRef({ onStart, onTarget, onDrop, onCancel })
  callbacksRef.current = { onStart, onTarget, onDrop, onCancel }

  useEffect(() => {
    const resolveTarget = (clientX: number, clientY: number): PointerSortTarget | null => {
      const element = document.elementFromPoint(clientX, clientY)?.closest<HTMLElement>(TARGET_SELECTOR)
      if (!element) return null
      const id = element.dataset.filetermSortId
      const kind = element.dataset.filetermSortKind
      return id && kind ? { id, kind, element } : null
    }

    const removeGhost = () => {
      ghostRef.current?.remove()
      ghostRef.current = null
      document.documentElement.classList.remove('fileterm-pointer-sorting')
    }

    const moveGhost = (state: NonNullable<PointerSortState<T>>, clientX: number, clientY: number) => {
      const ghost = ghostRef.current
      if (!ghost) return
      ghost.style.transform = `translate3d(${Math.round(state.originLeft + clientX - state.dragStartX)}px, ${Math.round(state.originTop + clientY - state.dragStartY)}px, 0)`
    }

    const createGhost = (sourceElement: HTMLElement, clientX: number, clientY: number) => {
      removeGhost()
      const rect = sourceElement.getBoundingClientRect()
      const computed = window.getComputedStyle(sourceElement)
      const ghost = sourceElement.cloneNode(true) as HTMLElement
      ghost.classList.add('fileterm-pointer-sort-ghost')
      ghost.setAttribute('aria-hidden', 'true')
      Object.assign(ghost.style, {
        position: 'fixed',
        inset: '0 auto auto 0',
        display: computed.display,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
        gridTemplateColumns: computed.gridTemplateColumns,
        columnGap: computed.columnGap,
        padding: computed.padding,
        border: computed.border,
        borderRadius: computed.borderRadius,
        background: computed.background,
        color: computed.color,
        font: computed.font,
        boxSizing: computed.boxSizing,
        opacity: '0.78',
        pointerEvents: 'none',
        zIndex: '2147483647',
        overflow: 'hidden',
        boxShadow: '0 3px 10px rgb(0 0 0 / 18%)',
        transition: 'none',
        willChange: 'transform',
        // Set the first transform before inserting the clone. Appending it
        // at the fixed-position origin and moving it afterwards produces a
        // visible one-frame flash in WebKit/Tauri windows.
        transform: `translate3d(${Math.round(rect.left)}px, ${Math.round(rect.top)}px, 0)`
      })
      document.body.appendChild(ghost)
      ghostRef.current = ghost
      const state = stateRef.current
      if (state) moveGhost(state, clientX, clientY)
    }

    const handlePointerMove = (event: PointerEvent) => {
      const state = stateRef.current
      if (!state) return

      if (!state.active) {
        const dx = event.clientX - state.pressX
        const dy = event.clientY - state.pressY
        if (dx * dx + dy * dy < 25) return
        state.active = true
        // Match Chromium's native drag image: when dragging actually starts,
        // its left/top still match the source row; later movement translates
        // the image by the cursor's delta from this exact start frame.
        state.dragStartX = event.clientX
        state.dragStartY = event.clientY
        event.preventDefault()
        createGhost(state.sourceElement, event.clientX, event.clientY)
        document.documentElement.classList.add('fileterm-pointer-sorting')
        callbacksRef.current.onStart(state.source)
      }

      event.preventDefault()
      moveGhost(state, event.clientX, event.clientY)
      const target = resolveTarget(event.clientX, event.clientY)
      if (target) callbacksRef.current.onTarget(state.source, target, event.clientY)
    }

    const handlePointerUp = (event: PointerEvent) => {
      const state = stateRef.current
      if (!state) return
      stateRef.current = null
      if (state.sourceElement.hasPointerCapture(state.pointerId)) {
        state.sourceElement.releasePointerCapture(state.pointerId)
      }
      removeGhost()
      if (!state.active) return
      callbacksRef.current.onDrop(state.source, resolveTarget(event.clientX, event.clientY), event.clientY)
    }

    const handlePointerCancel = () => {
      const state = stateRef.current
      if (!state) return
      stateRef.current = null
      if (state.sourceElement.hasPointerCapture(state.pointerId)) {
        state.sourceElement.releasePointerCapture(state.pointerId)
      }
      removeGhost()
      callbacksRef.current.onCancel()
    }

    window.addEventListener('pointermove', handlePointerMove, true)
    window.addEventListener('pointerup', handlePointerUp, true)
    window.addEventListener('pointercancel', handlePointerCancel, true)
    window.addEventListener('lostpointercapture', handlePointerCancel, true)
    window.addEventListener('blur', handlePointerCancel)
    return () => {
      window.removeEventListener('pointermove', handlePointerMove, true)
      window.removeEventListener('pointerup', handlePointerUp, true)
      window.removeEventListener('pointercancel', handlePointerCancel, true)
      window.removeEventListener('lostpointercapture', handlePointerCancel, true)
      window.removeEventListener('blur', handlePointerCancel)
      removeGhost()
    }
  }, [])

  return (event: ReactPointerEvent, source: T) => {
    if (event.button !== 0 || !event.isPrimary) return
    if (event.target instanceof Element && event.target.closest(INTERACTIVE_SELECTOR)) return
    const rect = event.currentTarget.getBoundingClientRect()
    try {
      event.currentTarget.setPointerCapture(event.pointerId)
    } catch {
      // Window-level listeners remain the fallback on WebViews that reject
      // pointer capture for a synthetic or already-released pointer.
    }
    stateRef.current = {
      source,
      sourceElement: event.currentTarget as HTMLElement,
      pressX: event.clientX,
      pressY: event.clientY,
      originLeft: rect.left,
      originTop: rect.top,
      dragStartX: event.clientX,
      dragStartY: event.clientY,
      pointerId: event.pointerId,
      active: false
    }
  }
}
