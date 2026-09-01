import { APP_EVENT, onAppEvent } from '../lib/app-events'
import {
  describeTerminalInput,
  getVimVisualSelection,
  isTerminalClipboardShortcut,
  logTerminalClipboard,
  logTerminalRender,
  logTerminalZoom,
  PinchGestureEvent,
  TERMINAL_GESTURE_ZOOM_THRESHOLD,
  TERMINAL_WHEEL_ZOOM_THRESHOLD
} from './terminal-view-utils'
import type {
  TerminalLifecycleInputHandlers,
  TerminalLifecycleResizeHandlers,
  TerminalLifecycleRuntime
} from './terminal-lifecycle-types'
import type { TerminalZoomOperation } from '@fileterm/core'

export function registerTerminalInteractionHandlers(
  runtime: TerminalLifecycleRuntime,
  resizeHandlers: TerminalLifecycleResizeHandlers,
  inputHandlers: TerminalLifecycleInputHandlers
) {
  const {
    terminal,
    terminalTextarea,
    hostRef,
    isMac,
    isActiveRef,
    lastObservedHostRectRef,
    tabIdRef,
    findOpenRef,
    setHasSelection,
    setContextMenu,
    runCopy,
    runPaste,
    openFind,
    closeFind,
    applyTerminalZoom,
    getTerminalZoomOperation,
    markTerminalFocused,
    markTerminalUnderPointer,
    clearTerminalUnderPointer,
    isLastFocusedTerminal,
    isTerminalUnderPointer
  } = runtime
  const { onWindowFocus, onVisibilityChange } = resizeHandlers
  const { resetInputState } = inputHandlers
  const host = hostRef.current

  const openContextMenu = (event: MouseEvent) => {
    event.preventDefault()
    // This is a document-capture handler, so prevent xterm.js and any native
    // context-menu handler from observing the canonical event after us.
    // The earlier pointerdown/mousedown capture handlers are responsible for
    // stopping xterm's mouse-reporting handlers.
    event.stopImmediatePropagation()
    // Read selection state when the menu is about to show: xterm.js finishes
    // its selection on pointerup, so at contextmenu time terminal.hasSelection()
    // is authoritative. Pointerdown/mousedown are too early.
    // Keep the terminal as the focus owner while the portal menu is open so
    // a menu click cannot make a paste target ambiguous on WebKitGTK.
    markTerminalFocused()
    terminal.focus()
    const vimVisualSelection = getVimVisualSelection(terminal, true)
    const nextHasSelection = terminal.hasSelection() || Boolean(vimVisualSelection)
    setHasSelection(nextHasSelection)
    logTerminalClipboard(terminal, 'context-menu-opened', {
      nextHasSelection,
      source: vimVisualSelection ? 'vim-visual' : 'xterm',
      vimVisualMode: vimVisualSelection?.mode,
      vimVisualRows: vimVisualSelection ? `${vimVisualSelection.startRow}-${vimVisualSelection.endRow}` : undefined
    })
    setContextMenu({ x: event.clientX, y: event.clientY })
  }

  const isSecondaryButton = (event: MouseEvent | PointerEvent) => {
    // macOS Ctrl+Click synthesizes a context menu with button 0 + ctrlKey.
    return event.button === 2 || (event.button === 0 && event.ctrlKey && isMac)
  }
  const isEventInsideTerminal = (event: Event) => {
    const target = event.target
    if (!host) {
      return false
    }
    if (target instanceof Node && host.contains(target)) {
      return true
    }
    // In WebView2 and WebKitGTK, compositor-backed terminal canvases can
    // retarget a wheel event during capture. The composed path still retains
    // the terminal host, so use it as the platform-safe fallback.
    return event.composedPath().includes(host)
  }
  const isEventOverTerminal = (event: Event) => {
    const { clientX, clientY } = event as MouseEvent
    if (!host || !Number.isFinite(clientX) || !Number.isFinite(clientY)) {
      return false
    }
    const bounds = lastObservedHostRectRef.current
    if (!bounds) {
      return false
    }
    return clientX >= bounds.left && clientX <= bounds.right && clientY >= bounds.top && clientY <= bounds.bottom
  }
  const isRootRetargetedGesture = (event: Event) => {
    const target = event.target
    return (
      target === null ||
      target === window ||
      target === document ||
      target === document.documentElement ||
      target === document.body
    )
  }
  const isTerminalGestureTarget = (event: Event) => {
    if (!isActiveRef.current) {
      return false
    }
    if (isEventInsideTerminal(event)) {
      return true
    }

    // WebView2, WebKitGTK and WKWebView can dispatch a compositor gesture
    // at the document root rather than the element beneath the pinch. Only
    // those root-retargeted events need the geometry/focus fallback. A
    // regular file-pane wheel event must stay on the native file scroller.
    if (!isRootRetargetedGesture(event)) {
      return false
    }
    if (isEventOverTerminal(event)) {
      return true
    }
    const { clientX, clientY } = event as MouseEvent
    if (Number.isFinite(clientX) && Number.isFinite(clientY)) {
      return false
    }

    // The xterm textarea is the authoritative focus owner when the WebView
    // drops both the target and composed path. A click into another pane
    // updates lastFocusedTerminal before its next gesture.
    return isLastFocusedTerminal() && document.activeElement === terminalTextarea
  }
  const isTerminalShortcutTarget = (event: KeyboardEvent) =>
    isActiveRef.current &&
    isLastFocusedTerminal() &&
    (document.activeElement === terminalTextarea || isEventInsideTerminal(event))

  const onMouseDown = (event: MouseEvent) => {
    if (!isSecondaryButton(event)) {
      return
    }
    // Vim and other TUI programs enable xterm mouse reporting. xterm.js
    // registers its own mousedown listener that calls ev.preventDefault() +
    // this.focus(), suppressing the contextmenu event and sending a mouse
    // sequence to the remote program. Stop it here so the native contextmenu
    // event still fires and our menu can open.
    event.stopImmediatePropagation()
  }
  const onPointerDown = (event: PointerEvent) => {
    if (!isSecondaryButton(event)) {
      return
    }
    // xterm.js 5.x uses Pointer Events for mouse reporting. Intercept the
    // right button in the capture phase before xterm converts it into an
    // escape sequence for Vim/nano. Do not call preventDefault(): that would
    // break xterm.js text selection. Do not set menu state here: selection is
    // not final until pointerup/contextmenu.
    event.stopImmediatePropagation()
  }

  let controlKeyActive = false
  let wheelZoomDelta = 0
  const normalizeWheelZoomDelta = (event: WheelEvent) => {
    const rawDelta = event.deltaY
    if (!Number.isFinite(rawDelta) || rawDelta === 0) {
      return 0
    }
    const delta =
      event.deltaMode === WheelEvent.DOM_DELTA_LINE
        ? rawDelta * 4
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
          ? rawDelta * TERMINAL_WHEEL_ZOOM_THRESHOLD
          : rawDelta
    return Math.max(-TERMINAL_WHEEL_ZOOM_THRESHOLD, Math.min(TERMINAL_WHEEL_ZOOM_THRESHOLD, delta))
  }
  const consumeTerminalWheelZoom = (event: WheelEvent, source: 'window' | 'xterm') => {
    // Chromium/WebKit surface both Ctrl+wheel and trackpad pinch as a wheel
    // event with ctrlKey=true. Some Windows/Linux WebViews lose ctrlKey on
    // compositor-retargeted wheel events, so retain the key state captured
    // by the window as a fallback. Keep this terminal-scoped: unmodified
    // two-finger scrolling continues to scroll the terminal normally.
    const hasZoomModifier = isMac
      ? event.ctrlKey || event.metaKey || event.getModifierState('Control') || event.getModifierState('Meta')
      : event.ctrlKey || event.getModifierState('Control') || controlKeyActive
    if (!hasZoomModifier) {
      logTerminalZoom(terminal, `wheel-${source}-ignored-no-modifier`, {
        ...describeTerminalInput(event),
        controlKeyActive,
        tabId: tabIdRef.current
      })
      return false
    }
    const delta = normalizeWheelZoomDelta(event)
    if (delta === 0) {
      logTerminalZoom(terminal, `wheel-${source}-ignored-zero-delta`, {
        ...describeTerminalInput(event),
        tabId: tabIdRef.current
      })
      return true
    }
    wheelZoomDelta += delta
    const steps = Math.floor(Math.abs(wheelZoomDelta) / TERMINAL_WHEEL_ZOOM_THRESHOLD)
    if (steps === 0) {
      logTerminalZoom(terminal, `wheel-${source}-accumulating`, {
        ...describeTerminalInput(event),
        delta,
        wheelZoomDelta,
        tabId: tabIdRef.current
      })
      return true
    }
    const direction = wheelZoomDelta < 0 ? 1 : -1
    wheelZoomDelta -= Math.sign(wheelZoomDelta) * steps * TERMINAL_WHEEL_ZOOM_THRESHOLD
    const changed = runtime.adjustTerminalFontSize(direction * steps)
    logTerminalZoom(terminal, `wheel-${source}-applied`, {
      ...describeTerminalInput(event),
      changed,
      direction,
      steps,
      tabId: tabIdRef.current
    })
    return true
  }
  const forwardRetargetedTerminalWheel = (event: WheelEvent) => {
    if (!isActiveRef.current || isEventInsideTerminal(event)) {
      return false
    }
    // WebView2/WebKitGTK can retarget a compositor wheel to the document
    // root. Re-dispatch it on xterm so xterm keeps the protocol decision:
    // normal-buffer scrollback, alternate-screen cursor-key fallback, or
    // application mouse reporting.
    const element = terminal.element
    if (!element) {
      return false
    }
    const forwardedEvent = new WheelEvent('wheel', {
      bubbles: true,
      cancelable: true,
      clientX: event.clientX,
      clientY: event.clientY,
      ctrlKey: event.ctrlKey,
      deltaMode: event.deltaMode,
      deltaX: event.deltaX,
      deltaY: event.deltaY,
      deltaZ: event.deltaZ,
      altKey: event.altKey,
      metaKey: event.metaKey,
      screenX: event.screenX,
      screenY: event.screenY,
      shiftKey: event.shiftKey
    })
    element.dispatchEvent(forwardedEvent)
    logTerminalRender(terminal, 'retargeted-wheel-forwarded', {
      bufferType: terminal.buffer.active.type,
      mouseTrackingMode: terminal.modes.mouseTrackingMode,
      tabId: tabIdRef.current
    })
    return true
  }
  const onWheel = (event: WheelEvent) => {
    const matchesTerminal = isTerminalGestureTarget(event)
    if (!matchesTerminal) {
      return
    }
    if (!consumeTerminalWheelZoom(event, 'window') && !forwardRetargetedTerminalWheel(event)) {
      return
    }
    event.preventDefault()
    event.stopImmediatePropagation()
  }
  // xterm sees the terminal canvas' wheel event before it turns it into
  // scrollback or a remote mouse-reporting sequence. This is the primary
  // path; the window listener above remains for platform-retargeted events.
  terminal.attachCustomWheelEventHandler((event) => {
    if (consumeTerminalWheelZoom(event, 'xterm')) {
      event.preventDefault()
      event.stopImmediatePropagation()
      return false
    }
    return true
  })

  let previousGestureScale = 1
  let gestureZoomDelta = 0
  const onGestureStart = (event: Event) => {
    if (!isTerminalGestureTarget(event)) {
      return
    }
    // WKWebView reports trackpad pinch through WebKit GestureEvents rather
    // than Chromium's ctrlKey wheel path. Cancelling it prevents WebView
    // page zoom and keeps the gesture local to this terminal.
    event.preventDefault()
    event.stopImmediatePropagation()
    previousGestureScale = (event as PinchGestureEvent).scale ?? 1
    gestureZoomDelta = 0
    logTerminalZoom(terminal, 'gesture-start', { scale: previousGestureScale })
  }
  const onGestureChange = (event: Event) => {
    if (!isTerminalGestureTarget(event)) {
      return
    }
    event.preventDefault()
    event.stopImmediatePropagation()
    const scale = (event as PinchGestureEvent).scale ?? previousGestureScale
    if (!Number.isFinite(scale) || scale <= 0 || scale === previousGestureScale) {
      return
    }
    gestureZoomDelta += Math.log(scale / previousGestureScale)
    previousGestureScale = scale
    const steps = Math.floor(Math.abs(gestureZoomDelta) / TERMINAL_GESTURE_ZOOM_THRESHOLD)
    if (steps === 0) {
      return
    }
    const direction = gestureZoomDelta > 0 ? 1 : -1
    gestureZoomDelta -= direction * steps * TERMINAL_GESTURE_ZOOM_THRESHOLD
    applyTerminalZoom(direction > 0 ? 'in' : 'out', 'gesture', steps)
    logTerminalZoom(terminal, 'gesture-change', { direction, scale, steps })
  }
  const onGestureEnd = (event: Event) => {
    if (!isTerminalGestureTarget(event)) {
      return
    }
    event.preventDefault()
    event.stopImmediatePropagation()
    logTerminalZoom(terminal, 'gesture-end')
    previousGestureScale = 1
    gestureZoomDelta = 0
  }

  const onDocumentContextMenu = (event: MouseEvent) => {
    // Final safety net for Vim/nano with mouse reporting enabled: pointerdown
    // and mousedown may be consumed by xterm's mouse handler before our
    // hostRef capture runs in some WKWebView configurations. The contextmenu
    // event is the canonical right-click event and always fires at document
    // level, so we catch it here and verify the click landed inside our
    // terminal before showing the menu.
    if (!isEventInsideTerminal(event)) {
      return
    }
    openContextMenu(event)
  }
  const onDocumentSelectionChange = () => {
    const selection = window.getSelection()
    const anchorNode = selection?.anchorNode
    if (selection && !selection.isCollapsed && anchorNode && host && !host.contains(anchorNode)) {
      terminal.clearSelection()
    }
  }
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Control') {
      controlKeyActive = true
    }
    const terminalZoomOperation = getTerminalZoomOperation(event)
    if (event.ctrlKey || event.metaKey || terminalZoomOperation) {
      logTerminalZoom(terminal, 'key-window-received', {
        ...describeTerminalInput(event),
        operation: terminalZoomOperation,
        isTerminalShortcutTarget: isTerminalShortcutTarget(event),
        tabId: tabIdRef.current
      })
    }
    if (terminalZoomOperation && isTerminalShortcutTarget(event)) {
      // Tauri's WebViews can consume browser-style zoom shortcuts before
      // xterm sees them. Capture the terminal-scoped fallback at window
      // level, including Ctrl+Shift+0 on Windows/Linux.
      event.preventDefault()
      event.stopImmediatePropagation()
      applyTerminalZoom(terminalZoomOperation, 'shortcut')
      return
    }

    const matchesCopy = isTerminalClipboardShortcut(event, isMac, 'copy')
    const matchesPaste = isTerminalClipboardShortcut(event, isMac, 'paste')
    const isFocusedTerminal = isTerminalShortcutTarget(event)
    if (
      isFocusedTerminal &&
      matchesCopy &&
      (terminal.hasSelection() || Boolean(getVimVisualSelection(terminal, true)))
    ) {
      const target = event.target
      const editableTarget = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement ? target : null
      const editableSelection =
        editableTarget && editableTarget !== terminalTextarea
          ? editableTarget.selectionStart !== editableTarget.selectionEnd
          : false
      const documentSelection = window.getSelection()
      const hasDocumentSelection = Boolean(
        documentSelection && !documentSelection.isCollapsed && documentSelection.toString()
      )
      if (!editableSelection && !hasDocumentSelection) {
        event.preventDefault()
        event.stopImmediatePropagation()
        logTerminalClipboard(terminal, 'copy-shortcut-window', { key: event.key })
        runCopy()
      }
      return
    }
    if (isFocusedTerminal && matchesPaste) {
      const target = event.target
      const isExternalEditableTarget =
        target instanceof HTMLInputElement ||
        (target instanceof HTMLTextAreaElement && target !== terminalTextarea) ||
        (target instanceof HTMLElement && target.isContentEditable)
      if (!isExternalEditableTarget) {
        event.preventDefault()
        event.stopImmediatePropagation()
        logTerminalClipboard(terminal, 'paste-shortcut-window', { key: event.key })
        void runPaste()
      }
      return
    }
  }
  const onKeyUp = (event: KeyboardEvent) => {
    // WebKit can retarget keyup after an IME/textarea input event. These
    // flags belong to this terminal instance, so reset them for the
    // physical key regardless of the event target.
    if (event.key === 'Shift' || /^Digit[0-9]$/.test(event.code)) {
      resetInputState()
    }
    if (event.key === 'Control') {
      controlKeyActive = false
    }
  }
  const onWindowBlur = () => {
    // A keyup can be delivered to a different native surface after the app
    // loses focus. Do not let a stale Ctrl state turn later touchpad scrolls
    // into zoom gestures.
    controlKeyActive = false
    resetInputState()
  }

  const handleFocusTerminal = (targetTabId: string) => {
    if (!isActiveRef.current || (targetTabId && targetTabId !== tabIdRef.current)) {
      return
    }
    markTerminalFocused()
    terminal.focus()
  }
  const handleTerminalCopy = () => {
    if (!isActiveRef.current || !isLastFocusedTerminal()) {
      return
    }
    runCopy()
  }
  const handleTerminalPaste = () => {
    if (!isActiveRef.current || !isLastFocusedTerminal()) {
      return
    }
    void runPaste()
  }
  const handleTerminalFind = () => {
    if (!isActiveRef.current) {
      return
    }
    if (findOpenRef.current) {
      closeFind()
    } else {
      openFind()
    }
  }
  const handleTerminalZoom = (operation: TerminalZoomOperation) => {
    if (!isActiveRef.current || !isLastFocusedTerminal()) {
      logTerminalZoom(terminal, 'native-zoom-request-ignored-not-focused', {
        operation,
        tabId: tabIdRef.current
      })
      return
    }
    logTerminalZoom(terminal, 'native-zoom-request-received', { operation, tabId: tabIdRef.current })
    applyTerminalZoom(operation, 'menu')
  }
  const handleTerminalGestureZoom = (operation: TerminalZoomOperation) => {
    if (!isActiveRef.current || !isTerminalUnderPointer()) {
      logTerminalZoom(terminal, 'native-gesture-zoom-ignored-not-hovered', {
        operation,
        hoveredTerminal: isTerminalUnderPointer() ? 'this-terminal' : 'another-terminal'
      })
      return
    }
    logTerminalZoom(terminal, 'native-gesture-zoom-received', { operation, tabId: tabIdRef.current })
    applyTerminalZoom(operation, 'gesture')
  }

  if (host) {
    // xterm's textarea focus event is not consistently forwarded after a
    // compositor-backed pointer interaction. Record the active pane from the
    // host's capture phase as well, before xterm consumes the event.
    host.addEventListener('focusin', markTerminalFocused)
    host.addEventListener('pointerdown', markTerminalFocused, true)
    host.addEventListener('pointerenter', markTerminalUnderPointer)
    host.addEventListener('pointerleave', clearTerminalUnderPointer)
    host.addEventListener('mousedown', onMouseDown, true)
    host.addEventListener('pointerdown', onPointerDown, true)
  }
  // Register at window capture so Windows WebView2 and Linux WebKitGTK
  // compositor events cannot bypass a listener attached below xterm's DOM.
  window.addEventListener('wheel', onWheel, { capture: true, passive: false })
  // GestureEvents can target document/body in WKWebView, so listen at the
  // window capture phase and scope the handler with isTerminalGestureTarget.
  window.addEventListener('gesturestart', onGestureStart, { capture: true, passive: false })
  window.addEventListener('gesturechange', onGestureChange, { capture: true, passive: false })
  window.addEventListener('gestureend', onGestureEnd, { capture: true, passive: false })
  document.addEventListener('contextmenu', onDocumentContextMenu, true)
  window.addEventListener('keydown', onKeyDown, true)
  window.addEventListener('keyup', onKeyUp, true)
  window.addEventListener('blur', onWindowBlur)
  window.addEventListener('focus', onWindowFocus)
  document.addEventListener('selectionchange', onDocumentSelectionChange)
  document.addEventListener('visibilitychange', onVisibilityChange)
  const offFocusTerminal = onAppEvent(APP_EVENT.focusTerminal, handleFocusTerminal)
  const offTerminalCopy = onAppEvent(APP_EVENT.terminalCopy, handleTerminalCopy)
  const offTerminalPaste = onAppEvent(APP_EVENT.terminalPaste, handleTerminalPaste)
  const offTerminalFind = onAppEvent(APP_EVENT.terminalFind, handleTerminalFind)
  const offTerminalZoom = onAppEvent(APP_EVENT.terminalZoom, handleTerminalZoom)
  const offNativeTerminalZoom = window.fileterm?.onTerminalZoomRequest(handleTerminalZoom)
  const offNativeTerminalGestureZoom = window.fileterm?.onTerminalGestureZoomRequest(handleTerminalGestureZoom)

  return {
    dispose: () => {
      offFocusTerminal()
      offTerminalCopy()
      offTerminalPaste()
      offTerminalFind()
      offTerminalZoom()
      offNativeTerminalZoom?.()
      offNativeTerminalGestureZoom?.()
      host?.removeEventListener('focusin', markTerminalFocused)
      host?.removeEventListener('pointerdown', markTerminalFocused, true)
      host?.removeEventListener('pointerenter', markTerminalUnderPointer)
      host?.removeEventListener('pointerleave', clearTerminalUnderPointer)
      host?.removeEventListener('mousedown', onMouseDown, true)
      host?.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('wheel', onWheel, true)
      window.removeEventListener('gesturestart', onGestureStart, true)
      window.removeEventListener('gesturechange', onGestureChange, true)
      window.removeEventListener('gestureend', onGestureEnd, true)
      document.removeEventListener('contextmenu', onDocumentContextMenu, true)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('keyup', onKeyUp, true)
      window.removeEventListener('blur', onWindowBlur)
      window.removeEventListener('focus', onWindowFocus)
      document.removeEventListener('selectionchange', onDocumentSelectionChange)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      runtime.clearGlobalTerminalState()
    }
  }
}
