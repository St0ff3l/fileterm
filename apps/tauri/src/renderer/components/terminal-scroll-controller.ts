import type { Terminal } from '@xterm/xterm'
import type { VerticalScrollController } from '../features/common/vertical-scrollbar'

export function createTerminalScrollController(
  terminalRef: { current: Terminal | null },
  element: HTMLElement
): VerticalScrollController {
  return {
    getElement: () => element,
    getMetrics: (observedHeight) => {
      const terminal = terminalRef.current
      if (!terminal) return null
      const rows = Math.max(1, terminal.rows)
      const clientHeight = observedHeight ?? element.clientHeight
      if (clientHeight <= 0) return null
      const buffer = terminal.buffer.active
      const scrollHeight = Math.max(clientHeight, (clientHeight * Math.max(rows, buffer.length)) / rows)
      const maxScrollTop = Math.max(0, scrollHeight - clientHeight)
      const scrollTop = Math.max(0, Math.min(maxScrollTop, (clientHeight * Math.max(0, buffer.viewportY)) / rows))
      return { clientHeight, scrollHeight, scrollTop }
    },
    scrollTo: (scrollTop) => {
      const terminal = terminalRef.current
      const clientHeight = element.clientHeight
      if (!terminal || clientHeight <= 0) return
      terminal.scrollToLine(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
    },
    scrollBy: (scrollTop) => {
      const terminal = terminalRef.current
      const clientHeight = element.clientHeight
      if (!terminal || clientHeight <= 0) return
      terminal.scrollLines(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
    },
    subscribe: (listener) => {
      const terminal = terminalRef.current
      if (!terminal) return () => undefined
      let pending = false
      let disposed = false
      let previousRows = terminal.rows
      let previousLength = terminal.buffer.active.length
      let previousViewportY = terminal.buffer.active.viewportY
      const notify = () => {
        if (pending || disposed || terminalRef.current !== terminal) return
        pending = true
        queueMicrotask(() => {
          pending = false
          if (disposed || terminalRef.current !== terminal) return
          const buffer = terminal.buffer.active
          // Fullscreen redraws and output at the history limit can leave scroll metrics unchanged.
          if (
            previousRows === terminal.rows &&
            previousLength === buffer.length &&
            previousViewportY === buffer.viewportY
          )
            return
          previousRows = terminal.rows
          previousLength = buffer.length
          previousViewportY = buffer.viewportY
          listener()
        })
      }
      // Cursor/selection paints do not change scroll metrics; parser events already cover new output.
      const disposables = [
        terminal.onScroll(notify),
        terminal.onResize(notify),
        terminal.onWriteParsed(notify),
        terminal.buffer.onBufferChange(notify)
      ]
      return () => {
        disposed = true
        disposables.forEach((disposable) => disposable.dispose())
      }
    }
  }
}
