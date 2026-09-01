import type { TerminalLifecycleInputHandlers, TerminalLifecycleRuntime } from './terminal-lifecycle-types'
import {
  describeTerminalInput,
  getShiftedTerminalInput,
  isFocusTrackingSequence,
  isShiftedTerminalInputData,
  isTerminalClipboardShortcut,
  isTerminalFindShortcut,
  isTerminalResponseSequence,
  logTerminalClipboard,
  logTerminalZoom,
  stripHydratedTerminalResponses,
  TERMINAL_IME_DUPLICATE_WINDOW_MS
} from './terminal-view-utils'

type PendingTerminalInputEvent = {
  event: InputEvent
  data: string
  xtermEmitted: boolean
  kind: 'shifted' | 'committed'
  beforeKeydown: boolean
}

type TerminalKeydownRecord = {
  data: string
  at: number
  xtermEmitted: boolean
}

export function registerTerminalInputHandlers(runtime: TerminalLifecycleRuntime): TerminalLifecycleInputHandlers {
  const {
    terminal,
    terminalTextarea,
    isMac,
    isActiveRef,
    connectedRef,
    connectingRef,
    serialTransferBusyRef,
    imeCompositionActiveRef,
    imeCompositionTextRef,
    imeCompositionEndedAtRef,
    imeCompositionDataForwardedRef,
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

  let suppressShiftedTerminalKeypress = false
  let shiftedTerminalInputSent = false
  let activeShiftedTerminalInputCode: string | null = null
  let lastCommittedShiftedInput: { data: string; at: number } | null = null
  let pendingTerminalInputEvent: PendingTerminalInputEvent | null = null
  const recentTerminalKeydowns: TerminalKeydownRecord[] = []
  const suppressedTerminalKeydowns: Array<{ data: string; at: number }> = []
  const TERMINAL_INPUT_EVENT_WINDOW_MS = 250

  const pruneTerminalInputEvents = (now = Date.now()) => {
    const cutoff = now - TERMINAL_INPUT_EVENT_WINDOW_MS
    while (recentTerminalKeydowns.length > 0 && recentTerminalKeydowns[0].at < cutoff) {
      recentTerminalKeydowns.shift()
    }
    while (suppressedTerminalKeydowns.length > 0 && suppressedTerminalKeydowns[0].at < cutoff) {
      suppressedTerminalKeydowns.shift()
    }
  }
  const getPrintableTerminalKey = (event: KeyboardEvent) => {
    if (
      event.ctrlKey ||
      event.metaKey ||
      event.altKey ||
      event.isComposing ||
      event.key === 'Dead' ||
      event.key === 'Process' ||
      event.key === 'Unidentified' ||
      [...event.key].length !== 1
    ) {
      return null
    }
    const codePoint = event.key.codePointAt(0) ?? 0
    return codePoint >= 0x20 && codePoint !== 0x7f ? event.key : null
  }
  const rememberTerminalKeydown = (event: KeyboardEvent) => {
    const data = getPrintableTerminalKey(event)
    if (!data) {
      return
    }
    const now = Date.now()
    pruneTerminalInputEvents(now)
    recentTerminalKeydowns.push({ data, at: now, xtermEmitted: false })
  }
  const markTerminalKeydownEmitted = (data: string) => {
    const now = Date.now()
    pruneTerminalInputEvents(now)
    const record = recentTerminalKeydowns.find(
      (candidate) =>
        candidate.data === data && !candidate.xtermEmitted && now - candidate.at <= TERMINAL_INPUT_EVENT_WINDOW_MS
    )
    if (record) {
      record.xtermEmitted = true
    }
  }
  const takeMatchingTerminalKeydown = (data: string) => {
    const now = Date.now()
    pruneTerminalInputEvents(now)
    const index = recentTerminalKeydowns.findIndex(
      (candidate) => candidate.data === data && now - candidate.at <= TERMINAL_INPUT_EVENT_WINDOW_MS
    )
    return index >= 0 ? recentTerminalKeydowns.splice(index, 1)[0] : null
  }
  const consumeSuppressedTerminalKeydown = (data: string) => {
    const now = Date.now()
    pruneTerminalInputEvents(now)
    const index = suppressedTerminalKeydowns.findIndex(
      (candidate) => candidate.data === data && now - candidate.at <= TERMINAL_INPUT_EVENT_WINDOW_MS
    )
    if (index < 0) {
      return false
    }
    suppressedTerminalKeydowns.splice(index, 1)
    return true
  }
  const resetShiftedTerminalInputState = () => {
    shiftedTerminalInputSent = false
    activeShiftedTerminalInputCode = null
    lastCommittedShiftedInput = null
    suppressShiftedTerminalKeypress = false
  }
  const resetInputState = () => {
    resetShiftedTerminalInputState()
    pendingTerminalInputEvent = null
    recentTerminalKeydowns.length = 0
    suppressedTerminalKeydowns.length = 0
  }

  terminal.attachCustomKeyEventHandler((event) => {
    if (event.type === 'keypress') {
      if (!suppressShiftedTerminalKeypress) {
        return true
      }

      // Some WebKit builds still dispatch keypress after a canceled
      // keydown. The character was already sent by the compatibility path;
      // suppress this legacy xterm route to avoid a duplicate byte.
      suppressShiftedTerminalKeypress = false
      event.preventDefault()
      event.stopPropagation()
      return false
    }

    if (event.type !== 'keydown') {
      return true
    }

    // If WebKit drops the previous keyup, a normal number-row keydown is a
    // safe boundary for the Shift-symbol compatibility state. Without this
    // fallback, a later 4/5/6 can inherit the previous symbol's one-shot
    // suppression flag and appear to be swallowed.
    if (!event.shiftKey && /^Digit[0-9]$/.test(event.code)) {
      resetShiftedTerminalInputState()
    }

    if (!shiftedTerminalInputSent) {
      suppressShiftedTerminalKeypress = false
    }

    const shiftedTerminalInput = getShiftedTerminalInput(event)
    if (shiftedTerminalInput && connectedRef.current) {
      const hasRecentMatchingInput =
        lastCommittedShiftedInput !== null &&
        lastCommittedShiftedInput.data === shiftedTerminalInput &&
        Date.now() - lastCommittedShiftedInput.at <= 100
      const isNewPhysicalKey = event.repeat || activeShiftedTerminalInputCode !== event.code
      if (isNewPhysicalKey && !hasRecentMatchingInput) {
        shiftedTerminalInputSent = false
        suppressShiftedTerminalKeypress = false
      }
      activeShiftedTerminalInputCode = event.code
      lastCommittedShiftedInput = null

      // WebKit can lose the first Shift+number character in xterm's hidden
      // textarea path when the next key arrives quickly. Send the resolved
      // character through the same PTY boundary while preventing xterm from
      // processing this key a second time.
      event.preventDefault()
      event.stopPropagation()
      suppressShiftedTerminalKeypress = true
      if (!shiftedTerminalInputSent) {
        clearEphemeralHighlight()
        setContextMenu(null)
        writeTerminalInput(shiftedTerminalInput)
        shiftedTerminalInputSent = true
      }
      return false
    }

    const printableTerminalKey = getPrintableTerminalKey(event)
    if (printableTerminalKey && !isShiftedTerminalInputData(printableTerminalKey)) {
      if (consumeSuppressedTerminalKeydown(printableTerminalKey)) {
        // The platform already committed this character through the input
        // event before delivering keydown. Do not let xterm replay it via
        // evaluateKeyboardEvent or CompositionHelper.
        event.preventDefault()
        event.stopPropagation()
        return false
      }
      rememberTerminalKeydown(event)
    }

    // Let xterm's CompositionHelper receive IME key events. In particular,
    // keyCode 229 / Process events must not be mistaken for reconnect or
    // terminal shortcuts while a composition is being finalized.
    if (imeCompositionActiveRef.current || event.isComposing || event.keyCode === 229 || event.key === 'Process') {
      return true
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

    return true
  })

  const onDataDispose = terminal.onData((data) => {
    markTerminalKeydownEmitted(data)
    if (pendingTerminalInputEvent && data === pendingTerminalInputEvent.data) {
      // The xterm input listener runs before our post-input listener. This
      // marker lets the bridge distinguish a character xterm already
      // committed from one that needs to be forwarded by FileTerm.
      pendingTerminalInputEvent.xtermEmitted = true
    }

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
      const responseWrite = window.fileterm?.writeTerminal(tabIdRef.current, rawTerminalData)
      responseWrite?.catch((error: unknown) => {
        console.debug('[TerminalView] startup terminal response was not accepted', error)
      })
      return
    }
    if (!isActiveRef.current || !data) {
      return
    }

    const isStandaloneNumericInputDuringIme = /^[0-9]+$/.test(data) && imeCompositionTextRef.current.length === 0
    if (imeCompositionActiveRef.current && !isStandaloneNumericInputDuringIme) {
      // Intermediate composition text is owned by the IME and must never
      // reach the remote PTY before compositionend. A digit with no active
      // composition text is different: macOS Chinese IMEs can route a
      // standalone number through the composition/keyCode=229 path.
      return
    }

    const compositionText = imeCompositionTextRef.current
    const compositionEndedAt = imeCompositionEndedAtRef.current
    const isWithinCompositionWindow =
      compositionText.length > 0 && Date.now() - compositionEndedAt <= TERMINAL_IME_DUPLICATE_WINDOW_MS
    if (isWithinCompositionWindow && data === ' ') {
      // Switching from a Chinese IME back to an ASCII layout can leave the
      // candidate-confirmation space as a standalone xterm data event.
      return
    }
    const isAsciiCompositionPayload = /^[\x20-\x7e]*$/.test(compositionText) && /^[\x20-\x7e]*$/.test(data)
    const compositionTextWithoutSpaces = compositionText.replaceAll(' ', '')
    const dataWithoutSpaces = data.replaceAll(' ', '')
    const duplicatedCompositionText = `${compositionTextWithoutSpaces}${compositionTextWithoutSpaces}`
    const isCompositionPayload =
      isWithinCompositionWindow &&
      compositionTextWithoutSpaces.length > 0 &&
      (dataWithoutSpaces === compositionTextWithoutSpaces || dataWithoutSpaces === duplicatedCompositionText)
    if (isCompositionPayload) {
      if (imeCompositionDataForwardedRef.current) {
        return
      }
      imeCompositionDataForwardedRef.current = true
      data = dataWithoutSpaces === duplicatedCompositionText ? compositionTextWithoutSpaces : dataWithoutSpaces
    } else if (
      isWithinCompositionWindow &&
      isAsciiCompositionPayload &&
      data.includes(' ') &&
      compositionTextWithoutSpaces.length > 0 &&
      data.length <= compositionText.length + 4 &&
      (dataWithoutSpaces.includes(compositionTextWithoutSpaces) ||
        compositionTextWithoutSpaces.includes(dataWithoutSpaces))
    ) {
      // The same transition may put the confirmation space inside the
      // payload (`W n`) instead of emitting it separately. Remove all spaces.
      data = dataWithoutSpaces
      if (!data) {
        return
      }
    }

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
    if (data.includes('\r') || data.includes('\n')) {
      runtime.awaitingCommandCompletionRef.current = true
    }
    const isFocusTrackingEvent = isFocusTrackingSequence(data)
    if (!isFocusTrackingEvent) {
      clearEphemeralHighlight()
      setContextMenu(null)
    } else {
      logTerminalClipboard(terminal, 'focus-tracking-preserved-selection', { data })
    }
    writeTerminalInput(data)
  })

  const onTerminalInputCapture = (event: Event) => {
    if (event.target !== terminalTextarea) {
      return
    }
    if (!isActiveRef.current) {
      event.preventDefault()
      event.stopImmediatePropagation()
      if (terminalTextarea) {
        terminalTextarea.value = ''
      }
      return
    }

    const inputEvent = event as InputEvent
    const data = inputEvent.data ?? ''
    if (inputEvent.inputType !== 'insertText' || !data) {
      return
    }

    const isShiftedInput =
      isShiftedTerminalInputData(data) && !inputEvent.isComposing && !imeCompositionActiveRef.current
    const isStandaloneNumericImeInput =
      /^[0-9]+$/.test(data) &&
      imeCompositionTextRef.current.length === 0 &&
      (imeCompositionActiveRef.current || inputEvent.isComposing)
    const isWithinCompositionWindow =
      imeCompositionTextRef.current.length > 0 &&
      Date.now() - imeCompositionEndedAtRef.current <= TERMINAL_IME_DUPLICATE_WINDOW_MS
    const isActualCompositionInput =
      (inputEvent.isComposing || imeCompositionActiveRef.current) && !isStandaloneNumericImeInput
    const isCommittedAsciiInput = /^[\x20-\x7e]+$/.test(data) && !isActualCompositionInput && !isWithinCompositionWindow
    if (!isShiftedInput && !isCommittedAsciiInput) {
      return
    }

    const keydownRecord = isShiftedInput ? null : takeMatchingTerminalKeydown(data)
    if (keydownRecord?.xtermEmitted) {
      // The normal keydown path already emitted this character. Prevent the
      // platform input event from entering xterm a second time.
      event.preventDefault()
      event.stopImmediatePropagation()
      if (terminalTextarea) {
        terminalTextarea.value = ''
      }
      return
    }
    pendingTerminalInputEvent = {
      event: inputEvent,
      data,
      xtermEmitted: false,
      kind: isShiftedInput ? 'shifted' : 'committed',
      beforeKeydown: !keydownRecord
    }
    if (isShiftedInput && shiftedTerminalInputSent) {
      // The keydown path already committed this character. Stop xterm's
      // input listener before it can emit a second copy.
      event.preventDefault()
      event.stopImmediatePropagation()
      pendingTerminalInputEvent = null
      if (terminalTextarea) {
        terminalTextarea.value = ''
      }
    }
  }

  const onTerminalInputAfterXterm = (event: Event) => {
    if (!isActiveRef.current) {
      pendingTerminalInputEvent = null
      if (terminalTextarea) {
        terminalTextarea.value = ''
      }
      return
    }
    const inputEvent = event as InputEvent
    if (pendingTerminalInputEvent?.event !== inputEvent) {
      return
    }

    const committedInput = pendingTerminalInputEvent
    pendingTerminalInputEvent = null
    // xterm's CompositionHelper keeps a deferred snapshot of this textarea.
    // Clear the committed character after xterm has observed it.
    if (terminalTextarea) {
      terminalTextarea.value = ''
    }

    if (committedInput.kind === 'shifted' && committedInput.xtermEmitted) {
      shiftedTerminalInputSent = true
      activeShiftedTerminalInputCode = null
      lastCommittedShiftedInput = { data: committedInput.data, at: Date.now() }
      suppressShiftedTerminalKeypress = true
      return
    }
    if (committedInput.kind === 'shifted' && shiftedTerminalInputSent) {
      return
    }
    if (committedInput.kind === 'committed' && committedInput.beforeKeydown) {
      const now = Date.now()
      pruneTerminalInputEvents(now)
      suppressedTerminalKeydowns.push({ data: committedInput.data, at: now })
    }
    if (committedInput.xtermEmitted || !connectedRef.current || serialTransferBusyRef.current) {
      return
    }

    clearEphemeralHighlight()
    setContextMenu(null)
    writeTerminalInput(committedInput.data)
    if (committedInput.kind === 'shifted') {
      shiftedTerminalInputSent = true
      activeShiftedTerminalInputCode = null
      lastCommittedShiftedInput = { data: committedInput.data, at: Date.now() }
      suppressShiftedTerminalKeypress = true
    }
  }

  window.addEventListener('input', onTerminalInputCapture, true)
  terminalTextarea?.addEventListener('input', onTerminalInputAfterXterm, true)

  return {
    onTerminalInputCapture,
    onTerminalInputAfterXterm,
    resetInputState,
    dispose: () => {
      onDataDispose.dispose()
      window.removeEventListener('input', onTerminalInputCapture, true)
      terminalTextarea?.removeEventListener('input', onTerminalInputAfterXterm, true)
      resetInputState()
    }
  }
}
