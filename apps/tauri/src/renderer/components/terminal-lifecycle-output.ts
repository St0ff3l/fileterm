import { readClipboardText, writeClipboardText } from '../app/app-utils'
import { observeCanvasTextMetrics } from '../app/font-metrics'
import { trimHydratedTerminalChunk } from '../app/terminal-transcript'
import {
  decodeBase64Utf8,
  encodeBase64Utf8,
  getLastVisibleTerminalLine,
  getVimVisualSelection,
  isOsc52TargetSupported,
  localizeTerminalText,
  logTerminalClipboard,
  logTerminalRender,
  looksLikeShellPrompt,
  splitOscPayload,
  TERMINAL_RESIZE_OUTPUT_QUIET_MS,
  TERMINAL_RESIZE_PIXEL_EPSILON,
  TERMINAL_RESIZE_SETTLE_MS
} from './terminal-view-utils'
import type { TerminalLifecycleResizeHandlers, TerminalLifecycleRuntime } from './terminal-lifecycle-types'

export function registerTerminalOutputHandlers(runtime: TerminalLifecycleRuntime): TerminalLifecycleResizeHandlers {
  const {
    terminal,
    fitAddon,
    searchAddon,
    hostRef,
    isActiveRef,
    tabIdRef,
    renderedTranscriptRef,
    suppressHydratedChunksUntilRef,
    replayingTranscriptRef,
    lastSyncedSizeRef,
    lastObservedHostRectRef,
    isHorizontalResizeActiveRef,
    lastTerminalOutputAtRef,
    awaitingCommandCompletionRef,
    pendingPromptResizeRef,
    pendingResizeForceRef,
    pendingResizeFreezeColsRef,
    resizeTimerRef,
    resizeSettleTimerRef,
    preserveVisibleBufferRef,
    connectedRef,
    connectingRef,
    wasConnectedRef,
    isReconnectingRef,
    inputSendFailedRef,
    reconnectHintShownRef,
    activeTerminalTabIdRef,
    onStatusRef,
    onReconnectRef,
    closedMessageRef,
    reconnectHintRef,
    setHasSelection,
    setFindMatchCount,
    setActiveFindIndex,
    setFindMiss,
    shouldHydrateTranscript,
    replaceTerminalWithTranscript,
    appendRenderedTranscript,
    clearSearchDecorations,
    formatTerminalChunk,
    scheduleTerminalWrite,
    buildExitAlternateScreenSequence,
    snapshotTerminalBuffer,
    syncTerminalSize
  } = runtime
  const host = hostRef.current!

  // Ghostty invalidates its render state whenever the terminal switches
  // between the normal and alternate screen. OpenCode, Claude Code, Vim,
  // and similar TUIs rely on that screen heavily; keeping a stale WebView
  // canvas/atlas after a tab activation can otherwise leave the terminal
  // looking black until another external repaint occurs.
  let renderedBufferType = terminal.buffer.active.type
  const screenBufferRenderDisposable = terminal.onWriteParsed(() => {
    const nextBufferType = terminal.buffer.active.type
    if (nextBufferType === renderedBufferType) {
      return
    }

    const previousBufferType = renderedBufferType
    renderedBufferType = nextBufferType
    terminal.clearTextureAtlas()
    terminal.refresh(0, Math.max(terminal.rows - 1, 0))
    logTerminalRender(terminal, 'screen-buffer-switched', {
      tabId: tabIdRef.current,
      previousBufferType,
      nextBufferType
    })
  })

  const searchResultsDisposable = searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
    setFindMatchCount(resultCount)
    setActiveFindIndex(resultIndex)
    setFindMiss(resultCount === 0)
  })

  const osc52Disposable = terminal.parser.registerOscHandler(52, async (payload) => {
    const parsed = splitOscPayload(payload)
    if (!parsed || !isOsc52TargetSupported(parsed.target)) {
      return false
    }

    if (replayingTranscriptRef.current) {
      return true
    }

    if (parsed.data === '?') {
      let clipboardText = ''
      try {
        clipboardText = await readClipboardText()
      } catch {
        clipboardText = ''
      }
      const encoded = encodeBase64Utf8(clipboardText)
      await window.fileterm?.writeTerminal(tabIdRef.current, `\u001b]52;${parsed.target || 'c'};${encoded}\u0007`)
      return true
    }

    const decoded = decodeBase64Utf8(parsed.data)
    if (decoded === null) {
      return false
    }

    await writeClipboardText(decoded)
    return true
  })

  syncTerminalSize(fitAddon, terminal)
  activeTerminalTabIdRef.current = tabIdRef.current

  const resize = (force = false, freezeCols = false, preserveVisibleBuffer = false) => {
    syncTerminalSize(fitAddon, terminal, { force, freezeCols, preserveVisibleBuffer })
  }
  const scheduleResize = (force = false, freezeCols = false, preserveVisibleBuffer = false) => {
    if (!isActiveRef.current) {
      return
    }
    pendingResizeForceRef.current = pendingResizeForceRef.current || force
    pendingResizeFreezeColsRef.current = pendingResizeFreezeColsRef.current || freezeCols

    if (resizeTimerRef.current !== null) {
      window.cancelAnimationFrame(resizeTimerRef.current)
    }

    resizeTimerRef.current = window.requestAnimationFrame(() => {
      resizeTimerRef.current = null
      const shouldForce = pendingResizeForceRef.current
      const shouldFreezeCols = pendingResizeFreezeColsRef.current
      pendingResizeForceRef.current = false
      pendingResizeFreezeColsRef.current = false
      resize(shouldForce, shouldFreezeCols, preserveVisibleBuffer)
    })
  }

  // Font loading and a monitor-DPI transition occur outside ResizeObserver's
  // CSS-box model. Refresh xterm's cached glyph/canvas metrics explicitly so
  // packaged WebView2/WebKit builds cannot keep a fallback-font grid.
  const disposeCanvasTextMetrics = observeCanvasTextMetrics((fontFamily) => {
    terminal.options.fontFamily = fontFamily
    terminal.clearTextureAtlas()
    terminal.refresh(0, Math.max(terminal.rows - 1, 0))
    scheduleResize(true)
  })

  const scheduleSettledHorizontalResize = () => {
    if (resizeSettleTimerRef.current !== null) {
      window.clearTimeout(resizeSettleTimerRef.current)
    }

    resizeSettleTimerRef.current = window.setTimeout(() => {
      const quietFor = Date.now() - lastTerminalOutputAtRef.current
      if (quietFor < TERMINAL_RESIZE_OUTPUT_QUIET_MS) {
        scheduleSettledHorizontalResize()
        return
      }

      if (awaitingCommandCompletionRef.current) {
        const promptLine = getLastVisibleTerminalLine(terminal)
        if (!looksLikeShellPrompt(promptLine)) {
          resizeSettleTimerRef.current = null
          isHorizontalResizeActiveRef.current = false
          pendingPromptResizeRef.current = true
          return
        }
        awaitingCommandCompletionRef.current = false
      }

      resizeSettleTimerRef.current = null
      isHorizontalResizeActiveRef.current = false
      pendingPromptResizeRef.current = false
      window.requestAnimationFrame(() => scheduleResize(true, false, true))
    }, TERMINAL_RESIZE_SETTLE_MS)
  }

  const resizeObserver = new ResizeObserver(() => {
    const host = hostRef.current
    if (!host) {
      return
    }

    const { left, top, right, bottom, width, height } = host.getBoundingClientRect()
    const lastObservedRect = lastObservedHostRectRef.current
    lastObservedHostRectRef.current = { left, top, right, bottom, width, height }

    const hostBecameUsable =
      isActiveRef.current &&
      width > 0 &&
      height > 0 &&
      Boolean(!lastObservedRect || lastObservedRect.width <= 0 || lastObservedRect.height <= 0)
    if (hostBecameUsable) {
      // WebKitGTK/WKWebView can report the hidden keep-alive surface as
      // zero-sized first and a usable box only after the visibility change.
      // Rebuild the cached glyph atlas at that boundary instead of waiting
      // for another terminal write to make the missing canvas repaint.
      terminal.clearTextureAtlas()
      scheduleResize(true)
      window.requestAnimationFrame(() => {
        if (isActiveRef.current) {
          terminal.refresh(0, Math.max(terminal.rows - 1, 0))
        }
      })
      return
    }

    const widthChanged = Boolean(
      lastObservedRect && Math.abs(lastObservedRect.width - width) > TERMINAL_RESIZE_PIXEL_EPSILON
    )
    if (widthChanged) {
      isHorizontalResizeActiveRef.current = true
      scheduleResize(false, true)
      scheduleSettledHorizontalResize()
      return
    }
    if (isHorizontalResizeActiveRef.current) {
      scheduleResize(false, true)
      scheduleSettledHorizontalResize()
      return
    }

    scheduleResize()
  })
  resizeObserver.observe(host)

  const onWindowFocus = () => {
    window.requestAnimationFrame(() => scheduleResize(true))
  }
  const onVisibilityChange = () => {
    if (document.visibilityState === 'visible') {
      window.requestAnimationFrame(() => scheduleResize(true))
    }
  }

  const registerSessionEvents = () => {
    const onSelectionDispose = terminal.onSelectionChange(() => {
      const nextHasSelection = terminal.hasSelection() || Boolean(getVimVisualSelection(terminal))
      setHasSelection(nextHasSelection)
      logTerminalClipboard(terminal, 'selection-changed', { nextHasSelection })
    })

    const offData = window.fileterm?.onTerminalData(({ tabId: nextTabId, chunk }) => {
      if (nextTabId !== tabIdRef.current) {
        return
      }
      lastTerminalOutputAtRef.current = Date.now()
      const shouldTrimHydratedBacklog = Date.now() < suppressHydratedChunksUntilRef.current
      if (shouldTrimHydratedBacklog) {
        // IPC ordering can leave one queued batch behind an authoritative
        // snapshot. Consume that single backlog opportunity; repeated fuzzy
        // trimming could eat legitimate repeated terminal output.
        suppressHydratedChunksUntilRef.current = 0
      }
      const visibleChunk = shouldTrimHydratedBacklog
        ? trimHydratedTerminalChunk(renderedTranscriptRef.current, chunk)
        : chunk
      if (!visibleChunk) {
        return
      }
      appendRenderedTranscript(visibleChunk)
      clearSearchDecorations()
      scheduleTerminalWrite(formatTerminalChunk(terminal, visibleChunk))
      if (pendingPromptResizeRef.current) {
        scheduleSettledHorizontalResize()
      }
    })

    const offState = window.fileterm?.onTerminalState(
      ({ tabId: nextTabId, summary, transcript, connected, status }) => {
        if (nextTabId !== tabIdRef.current) {
          return
        }
        onStatusRef.current?.(localizeTerminalText(summary))
        connectedRef.current = connected
        connectingRef.current = status === 'connecting'
        const isDisconnecting = wasConnectedRef.current && !connected
        if (isDisconnecting) {
          preserveVisibleBufferRef.current = true
        }
        if (shouldHydrateTranscript(renderedTranscriptRef.current, transcript, connected)) {
          replaceTerminalWithTranscript(terminal, transcript)
        }
        if (!wasConnectedRef.current && connected) {
          lastSyncedSizeRef.current = null
          preserveVisibleBufferRef.current = false
          awaitingCommandCompletionRef.current = false
          pendingPromptResizeRef.current = false
          window.requestAnimationFrame(() => scheduleResize(true))
        }
        if (isDisconnecting) {
          terminal.write(buildExitAlternateScreenSequence(), () => {
            const visibleTranscript = snapshotTerminalBuffer(terminal)
            const reconnectHint = onReconnectRef.current ? `\r\n${reconnectHintRef.current}` : ''
            if (reconnectHint) {
              reconnectHintShownRef.current = true
            }
            const disconnectedTranscript = visibleTranscript
              ? `${visibleTranscript}\r\n${closedMessageRef.current}${reconnectHint}\r\n`
              : `${closedMessageRef.current}${reconnectHint}\r\n`
            replaceTerminalWithTranscript(terminal, disconnectedTranscript)
          })
        } else if (!connected && status === 'error') {
          // A reconnect command only starts the worker, so its Promise resolves
          // before a failed TCP/SSH handshake is known. The terminal state is
          // the authoritative failure signal for both initial and retry attempts.
          runtime.writeReconnectHint()
        }
        wasConnectedRef.current = connected
        if (connected) {
          isReconnectingRef.current = false
          inputSendFailedRef.current = false
          reconnectHintShownRef.current = false
        }
      }
    )

    return () => {
      onSelectionDispose.dispose()
      offData?.()
      offState?.()
    }
  }

  return {
    resize,
    scheduleResize,
    scheduleSettledHorizontalResize,
    onWindowFocus,
    onVisibilityChange,
    registerSessionEvents,
    dispose: () => {
      screenBufferRenderDisposable.dispose()
      searchResultsDisposable.dispose()
      osc52Disposable.dispose()
      disposeCanvasTextMetrics()
      resizeObserver.disconnect()
    }
  }
}
