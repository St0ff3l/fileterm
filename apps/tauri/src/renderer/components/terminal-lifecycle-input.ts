import { isTerminalResponseSequence, stripHydratedTerminalResponses } from '../xterm-adapters/terminal-responses'
import { createTerminalInputCompatibility } from '../xterm-adapters/terminal-input-compat'
import { trackRendererWork } from '../lib/renderer-work-log'
import type { TerminalLifecycleInputHandlers, TerminalLifecycleRuntime } from './terminal-lifecycle-types'
import {
  describeTerminalInput,
  isFocusTrackingSequence,
  isTerminalClipboardShortcut,
  isTerminalFindShortcut,
  logTerminalClipboard,
  logTerminalZoom
} from './terminal-view-utils'

export function registerTerminalInputHandlers(runtime: TerminalLifecycleRuntime): TerminalLifecycleInputHandlers {
  const {
    terminal,
    isMac,
    isActiveRef,
    connectedRef,
    connectingRef,
    serialTransferBusyRef,
    replayingTranscriptRef,
    tabIdRef,
    clearEphemeralHighlight,
    setContextMenu,
    findOpenRef,
    runCopy,
    runPaste,
    openFind,
    closeFind,
    runSplitPane,
    runClosePane,
    runCloseTab,
    applyTerminalZoom,
    getTerminalZoomOperation,
    requestReconnect,
    writeTerminalInput
  } = runtime

  const inputCompatibility = createTerminalInputCompatibility(runtime)

  terminal.attachCustomKeyEventHandler((event) => {
    const compatibilityResult = inputCompatibility.handleKeyEvent(event)
    if (compatibilityResult !== null) {
      return compatibilityResult
    }

    const isHistoryOpen = document.body.getAttribute('data-history-open') === 'true'
    if (
      isHistoryOpen &&
      (event.key === 'ArrowUp' ||
        event.key === 'ArrowDown' ||
        event.key === 'ArrowLeft' ||
        event.key === 'ArrowRight' ||
        event.key === 'Enter' ||
        event.key === 'Escape')
    ) {
      return false
    }

    if (event.key === 'Enter' && requestReconnect()) {
      event.preventDefault()
      event.stopPropagation()
      return false
    }

    const matchesCopy = isTerminalClipboardShortcut(event, isMac, 'copy')
    const matchesPaste = isTerminalClipboardShortcut(event, isMac, 'paste')
    const matchesFind = isTerminalFindShortcut(event)
    const terminalZoomOperation = getTerminalZoomOperation(event)
    if (event.ctrlKey || event.metaKey || terminalZoomOperation) {
      logTerminalZoom(terminal, 'key-xterm-received', {
        ...describeTerminalInput(event),
        operation: terminalZoomOperation,
        tabId: tabIdRef.current
      })
    }

    if (matchesCopy) {
      event.preventDefault()
      event.stopPropagation()
      logTerminalClipboard(terminal, 'copy-shortcut-xterm', { key: event.key })
      runCopy()
      return false
    }

    if (matchesPaste) {
      event.preventDefault()
      event.stopPropagation()
      void runPaste()
      return false
    }

    if (matchesFind) {
      event.preventDefault()
      event.stopPropagation()
      if (findOpenRef.current) {
        closeFind()
      } else {
        openFind()
      }
      return false
    }

    if (terminalZoomOperation) {
      event.preventDefault()
      event.stopPropagation()
      applyTerminalZoom(terminalZoomOperation, 'shortcut')
      return false
    }

    const matchesClose = isMac
      ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'w'
      : event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'w'
    if (matchesClose) {
      event.preventDefault()
      event.stopPropagation()
      if (runtime.canClosePaneRef.current) {
        runClosePane()
      } else {
        runCloseTab()
      }
      return false
    }

    const matchesSplitVertical = isMac
      ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'd'
      : event.altKey &&
        event.shiftKey &&
        (event.key === '+' || event.key === '=' || event.code === 'Equal' || event.code === 'NumpadAdd')
    const matchesSplitHorizontal = isMac
      ? event.metaKey && event.shiftKey && event.key.toLowerCase() === 'd'
      : event.altKey &&
        event.shiftKey &&
        (event.key === '-' || event.key === '_' || event.code === 'Minus' || event.code === 'NumpadSubtract')
    if (matchesSplitVertical) {
      event.preventDefault()
      event.stopPropagation()
      runSplitPane('row')
      return false
    }
    if (matchesSplitHorizontal) {
      event.preventDefault()
      event.stopPropagation()
      runSplitPane('column')
      return false
    }

    if (
      event.key === 'Control' ||
      event.key === 'Meta' ||
      event.key === 'Alt' ||
      (event.key === 'Dead' && (event.metaKey || event.ctrlKey || event.altKey))
    ) {
      return false
    }

    if (runtime.completionControllerRef.current?.key(event)) {
      event.preventDefault()
      event.stopPropagation()
      return false
    }
    return true
  })

  const onDataDispose = terminal.onData(
    trackRendererWork('terminal-input', (data: string) => {
      // Windows ConPTY may ask for the cursor position (`ESC[6n`) before the
      // renderer has finished mounting. xterm answers with a terminal
      // response, but the normal disconnected guard below would discard it.
      // Forward only protocol responses while the PTY is still connecting;
      // ordinary user input remains blocked until the backend reports ready.
      const rawTerminalData = data
      const isStartupTerminalResponse =
        !connectedRef.current &&
        connectingRef.current &&
        isTerminalResponseSequence(rawTerminalData, String.fromCharCode(27))
      if (replayingTranscriptRef.current) {
        data = stripHydratedTerminalResponses(data)
      }
      if (isStartupTerminalResponse) {
        writeTerminalInput(rawTerminalData, (error: unknown) => {
          console.debug('[TerminalView] startup terminal response was not accepted', error)
        })
        return
      }
      // Query replies are parser output, including in background tabs and during IME composition.
      // Do not let React UI updates or user-input filters interrupt the terminal protocol.
      if (
        connectedRef.current &&
        !serialTransferBusyRef.current &&
        isTerminalResponseSequence(data, String.fromCharCode(27))
      ) {
        writeTerminalInput(data)
        return
      }
      if (!isActiveRef.current || !data) {
        return
      }

      inputCompatibility.markDataEmitted(data)
      const compositionData = inputCompatibility.filterCompositionData(data)
      if (compositionData === null) {
        return
      }
      data = compositionData

      // The serial transfer worker owns the port while a protocol is active.
      // Drop xterm input before it reaches the backend so quick sends, paste,
      // and ordinary terminal typing cannot interleave protocol frames.
      if (serialTransferBusyRef.current) {
        return
      }

      // When disconnected, intercept Enter to trigger reconnect instead of
      // forwarding to the (dead) PTY. Ignore while a reconnect is in flight.
      if (!connectedRef.current) {
        if (data.includes('\r') || data.includes('\n')) requestReconnect()
        return
      }
      const isFocusTrackingEvent = isFocusTrackingSequence(data)
      if (!isFocusTrackingEvent) {
        clearEphemeralHighlight()
        setContextMenu(null)
      } else {
        logTerminalClipboard(terminal, 'focus-tracking-preserved-selection', { data })
      }
      if (!isFocusTrackingEvent) runtime.completionControllerRef.current?.input(data)
      writeTerminalInput(data)
    })
  )

  return {
    onTerminalInputCapture: inputCompatibility.onTerminalInputCapture,
    onTerminalInputAfterXterm: inputCompatibility.onTerminalInputAfterXterm,
    resetInputState: inputCompatibility.resetInputState,
    dispose: () => {
      onDataDispose.dispose()
      inputCompatibility.dispose()
    }
  }
}
