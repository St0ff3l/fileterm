import { useEffect, useMemo, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import type { VerticalScrollController } from '../features/common/vertical-scrollbar'
import { TerminalLogColorizer } from '../app/terminal-log-colorizer'
import { t } from '../i18n'
import { getConfiguredMonoFontFamily } from '../app/font-metrics'
import {
  getTerminalFontSize,
  hydrateTerminalFontSizes,
  subscribeTerminalFontSizes
} from '../app/terminal-font-size-store'
import { normalizeLocalTerminalStartupTranscript, type SplitPaneDirection } from './terminal-view-utils'
import { useTerminalLifecycle } from './use-terminal-lifecycle'
import { useTerminalViewActions } from './terminal-view-actions'

export type TerminalViewProps = {
  profileId: string
  tabId: string
  bootText: string
  sessionType?: 'ssh' | 'ftp' | 'telnet' | 'serial' | 'local'
  connected?: boolean
  connecting?: boolean
  isActive?: boolean
  onStatus?(message: string | null): void
  onReconnect?(): void | Promise<void>
  onActivate?(): void
  onSplitPane?(direction: SplitPaneDirection): void
  onClosePane?(): void
  onCloseTab?(): void
  canClosePane?: boolean
  closedMessage?: string
  reconnectHint?: string
}

// WKWebView and WebKitGTK may keep a newly-visible keep-alive surface at zero
// size for several compositor frames while the workspace transition settles.
// Keep the retry bounded so a genuinely unavailable surface cannot create a
// permanent animation-frame loop.
const TERMINAL_ACTIVATION_RECOVERY_ATTEMPTS = 16

export function useTerminalView({
  profileId,
  tabId,
  bootText,
  sessionType = 'ssh',
  connected = false,
  connecting = false,
  isActive = true,
  onStatus,
  onReconnect,
  onSplitPane,
  onClosePane,
  onCloseTab,
  canClosePane = false,
  closedMessage = t.terminalConnectionClosed,
  reconnectHint = t.pressEnterToReconnect
}: TerminalViewProps) {
  const hydratedBootText =
    sessionType === 'local' ? normalizeLocalTerminalStartupTranscript(bootText, window.fileterm?.platform) : bootText
  const hostRef = useRef<HTMLDivElement | null>(null)
  const [terminalScrollableElement, setTerminalScrollableElement] = useState<HTMLElement | null>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const terminalLogColorizerRef = useRef<TerminalLogColorizer | null>(null)
  const searchAddonRef = useRef<SearchAddon | null>(null)
  const findInputRef = useRef<HTMLInputElement | null>(null)
  const bootTextRef = useRef(hydratedBootText)
  const renderedTranscriptRef = useRef('')
  const pendingWriteRef = useRef('')
  const writeFrameRef = useRef<number | null>(null)
  const resizeTimerRef = useRef<number | null>(null)
  const resizeSettleTimerRef = useRef<number | null>(null)
  const pendingResizeForceRef = useRef(false)
  const pendingResizeFreezeColsRef = useRef(false)
  const isWritingRef = useRef(false)
  const suppressHydratedChunksUntilRef = useRef(0)
  const preserveVisibleBufferRef = useRef(false)
  const bootedTabs = useRef(new Set<string>())
  const wasConnectedRef = useRef(false)
  // 后端 worker 异常退出时 writeTerminal 会 reject；记录一次避免每个按键
  // 都刷一行提示，直到 terminal:state 重新同步后复位。
  const inputSendFailedRef = useRef(false)
  // xterm owns composition events internally, but a few native IMEs can still
  // produce a second `onData` event while switching input languages. Track
  // only the current composition transaction so normal repeated input and
  // paste are unaffected.
  const imeCompositionActiveRef = useRef(false)
  const imeCompositionTextRef = useRef('')
  const imeCompositionEndedAtRef = useRef(0)
  const imeCompositionDataForwardedRef = useRef(false)
  const replayingTranscriptRef = useRef(false)
  const transcriptReplayGenerationRef = useRef(0)
  // `onData` is registered once for the xterm instance.  Reading the prop
  // through a ref prevents a stale terminal-state event from a background
  // tab from swallowing keystrokes after this tab is brought back.
  const connectedRef = useRef(Boolean(connected))
  const connectingRef = useRef(Boolean(connecting))
  const isActiveRef = useRef(Boolean(isActive))
  const serialTransferBusyRef = useRef(false)
  const lastSyncedSizeRef = useRef<{ cols: number; rows: number; width: number; height: number } | null>(null)
  const lastObservedHostRectRef = useRef<{
    left: number
    top: number
    right: number
    bottom: number
    width: number
    height: number
  } | null>(null)
  const isHorizontalResizeActiveRef = useRef(false)
  const lastTerminalOutputAtRef = useRef(0)
  const awaitingCommandCompletionRef = useRef(false)
  const pendingPromptResizeRef = useRef(false)
  const profileIdRef = useRef(profileId)
  const tabIdRef = useRef(tabId)
  const onStatusRef = useRef(onStatus)
  const onReconnectRef = useRef(onReconnect)
  const closedMessageRef = useRef(closedMessage)
  const reconnectHintRef = useRef(reconnectHint)
  const onSplitPaneRef = useRef(onSplitPane)
  const onClosePaneRef = useRef(onClosePane)
  const onCloseTabRef = useRef(onCloseTab)
  const canClosePaneRef = useRef(canClosePane)
  const isReconnectingRef = useRef(false)
  const reconnectHintShownRef = useRef(false)
  const activeTerminalTabIdRef = useRef<string | null>(null)
  profileIdRef.current = profileId
  tabIdRef.current = tabId
  connectedRef.current = Boolean(connected)
  connectingRef.current = Boolean(connecting)
  isActiveRef.current = Boolean(isActive)
  onStatusRef.current = onStatus
  onReconnectRef.current = onReconnect
  closedMessageRef.current = closedMessage
  reconnectHintRef.current = reconnectHint
  onSplitPaneRef.current = onSplitPane
  onClosePaneRef.current = onClosePane
  onCloseTabRef.current = onCloseTab
  canClosePaneRef.current = canClosePane

  useEffect(() => {
    serialTransferBusyRef.current = false
    if (sessionType !== 'serial' || !window.fileterm?.onSerialTransferProgress) {
      return
    }
    return window.fileterm.onSerialTransferProgress((progress) => {
      if (progress.tabId === tabId) {
        serialTransferBusyRef.current = progress.status === 'running'
      }
    })
  }, [sessionType, tabId])
  const [hasSelection, setHasSelection] = useState(false)
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const findOpenRef = useRef(findOpen)
  useEffect(() => {
    findOpenRef.current = findOpen
  }, [findOpen])
  const [findQuery, setFindQuery] = useState('')
  const [findMiss, setFindMiss] = useState(false)
  const [findMatchCount, setFindMatchCount] = useState(0)
  const [activeFindIndex, setActiveFindIndex] = useState(-1)
  const [findCaseSensitive, setFindCaseSensitive] = useState(false)
  const [findRegex, setFindRegex] = useState(false)
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(false)
  const terminalZoomLockedRef = useRef(false)
  const isMac = window.fileterm?.platform === 'darwin'
  const isWindowsPty = sessionType === 'local' && window.fileterm?.platform === 'win32'

  const terminalScrollController = useMemo<VerticalScrollController | null>(() => {
    if (!terminalScrollableElement) {
      return null
    }

    return {
      getElement: () => terminalScrollableElement,
      getMetrics: () => {
        const terminal = terminalRef.current
        if (!terminal) {
          return null
        }

        const rows = Math.max(1, terminal.rows)
        const clientHeight = terminalScrollableElement.clientHeight
        if (clientHeight <= 0) {
          return null
        }

        const buffer = terminal.buffer.active
        const scrollHeight = Math.max(clientHeight, (clientHeight * Math.max(rows, buffer.length)) / rows)
        const maxScrollTop = Math.max(0, scrollHeight - clientHeight)
        const scrollTop = Math.max(0, Math.min(maxScrollTop, (clientHeight * Math.max(0, buffer.viewportY)) / rows))
        return { clientHeight, scrollHeight, scrollTop }
      },
      scrollTo: (scrollTop) => {
        const terminal = terminalRef.current
        const clientHeight = terminalScrollableElement.clientHeight
        if (!terminal || clientHeight <= 0) {
          return
        }

        terminal.scrollToLine(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
      },
      scrollBy: (scrollTop) => {
        const terminal = terminalRef.current
        const clientHeight = terminalScrollableElement.clientHeight
        if (!terminal || clientHeight <= 0) {
          return
        }

        terminal.scrollLines(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
      },
      subscribe: (listener) => {
        const terminal = terminalRef.current
        if (!terminal) {
          return () => undefined
        }

        const disposables = [
          terminal.onScroll(listener),
          terminal.onResize(listener),
          terminal.onRender(listener),
          terminal.onWriteParsed(listener),
          terminal.buffer.onBufferChange(listener)
        ]
        return () => disposables.forEach((disposable) => disposable.dispose())
      }
    }
  }, [terminalScrollableElement])

  terminalZoomLockedRef.current = terminalZoomLocked

  useEffect(() => {
    const desktopApi = window.fileterm
    if (!desktopApi) {
      return
    }

    let canceled = false
    void desktopApi
      .getUiPreferences()
      .then((preferences) => {
        if (!canceled) {
          terminalZoomLockedRef.current = preferences.terminalZoomLocked
          setTerminalZoomLocked(preferences.terminalZoomLocked)
        }
      })
      .catch(() => {
        // Keep the safe default (unlocked) when the preference cannot be read.
      })

    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) {
        terminalZoomLockedRef.current = preferences.terminalZoomLocked
        setTerminalZoomLocked(preferences.terminalZoomLocked)
      }
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [])

  useEffect(() => {
    void hydrateTerminalFontSizes()
  }, [])

  const {
    shortcuts,
    buildTerminalTheme,
    applyTerminalTheme,
    buildSearchOptions,
    clearFindSelection,
    clearSearchDecorations,
    clearEphemeralHighlight,
    applyTerminalFontSize,
    syncTerminalSize,
    recoverTerminalRender,
    replaceTerminalWithTranscript,
    shouldHydrateTranscript,
    formatTerminalChunk,
    appendRenderedTranscript,
    scheduleTerminalWrite,
    buildExitAlternateScreenSequence,
    snapshotTerminalBuffer,
    runCopy,
    runPaste,
    runSaveSessionLog,
    openFind,
    closeFind,
    runFind,
    runClear,
    runSplitPane,
    runClosePane,
    runCloseTab,
    searchTerminal
  } = useTerminalViewActions({
    isMac,
    findOpen,
    findQuery,
    findCaseSensitive,
    findRegex,
    isActiveRef,
    hostRef,
    terminalRef,
    fitAddonRef,
    terminalLogColorizerRef,
    searchAddonRef,
    findOpenRef,
    renderedTranscriptRef,
    pendingWriteRef,
    writeFrameRef,
    isWritingRef,
    suppressHydratedChunksUntilRef,
    preserveVisibleBufferRef,
    transcriptReplayGenerationRef,
    replayingTranscriptRef,
    lastSyncedSizeRef,
    tabIdRef,
    onStatusRef,
    onSplitPaneRef,
    onClosePaneRef,
    onCloseTabRef,
    setHasSelection,
    setContextMenu,
    setFindOpen,
    setFindQuery,
    setFindMatchCount,
    setActiveFindIndex,
    setFindMiss
  })

  useTerminalLifecycle({
    isMac,
    isWindowsPty,
    isActive,
    bootText: hydratedBootText,
    hostRef,
    setTerminalScrollableElement,
    terminalRef,
    fitAddonRef,
    terminalLogColorizerRef,
    searchAddonRef,
    bootTextRef,
    renderedTranscriptRef,
    pendingWriteRef,
    writeFrameRef,
    resizeTimerRef,
    resizeSettleTimerRef,
    pendingResizeForceRef,
    pendingResizeFreezeColsRef,
    isWritingRef,
    suppressHydratedChunksUntilRef,
    preserveVisibleBufferRef,
    bootedTabs,
    wasConnectedRef,
    inputSendFailedRef,
    imeCompositionActiveRef,
    imeCompositionTextRef,
    imeCompositionEndedAtRef,
    imeCompositionDataForwardedRef,
    replayingTranscriptRef,
    transcriptReplayGenerationRef,
    connectedRef,
    connectingRef,
    isActiveRef,
    serialTransferBusyRef,
    lastSyncedSizeRef,
    lastObservedHostRectRef,
    isHorizontalResizeActiveRef,
    lastTerminalOutputAtRef,
    awaitingCommandCompletionRef,
    pendingPromptResizeRef,
    profileIdRef,
    tabIdRef,
    onStatusRef,
    onReconnectRef,
    closedMessageRef,
    reconnectHintRef,
    canClosePaneRef,
    isReconnectingRef,
    reconnectHintShownRef,
    activeTerminalTabIdRef,
    findOpenRef,
    terminalZoomLockedRef,
    setHasSelection,
    setContextMenu,
    setFindMatchCount,
    setActiveFindIndex,
    setFindMiss,
    buildTerminalTheme,
    applyTerminalFontSize,
    syncTerminalSize,
    replaceTerminalWithTranscript,
    shouldHydrateTranscript,
    formatTerminalChunk,
    appendRenderedTranscript,
    scheduleTerminalWrite,
    buildExitAlternateScreenSequence,
    snapshotTerminalBuffer,
    clearSearchDecorations,
    clearEphemeralHighlight,
    runCopy,
    runPaste,
    openFind,
    closeFind,
    runSplitPane,
    runClosePane,
    runCloseTab
  })

  useEffect(() => {
    if (!isActive) {
      return
    }

    let disposed = false
    let recoveryAttempts = 0
    let recoveryFrame: number | null = null
    const attemptRecovery = () => {
      if (disposed || !isActiveRef.current) {
        return
      }

      recoveryAttempts += 1
      const shouldRetry = recoverTerminalRender('activation')
      if (shouldRetry && recoveryAttempts < TERMINAL_ACTIVATION_RECOVERY_ATTEMPTS) {
        recoveryFrame = window.requestAnimationFrame(attemptRecovery)
      }
    }

    const firstFrame = window.requestAnimationFrame(() => {
      recoveryFrame = window.requestAnimationFrame(attemptRecovery)
    })

    return () => {
      disposed = true
      window.cancelAnimationFrame(firstFrame)
      if (recoveryFrame !== null) {
        window.cancelAnimationFrame(recoveryFrame)
      }
    }
  }, [isActive, recoverTerminalRender])

  useEffect(() => {
    bootTextRef.current = hydratedBootText

    // connected 提升必须放在提前 return 之前。
    //
    // 切 tab 流程：tab A(connected) → tab B(未连接) → 切回 tab A。
    //   1. 切到 B 时 onTerminalState 的 connected=false 会把 wasConnectedRef 写成 false
    //   2. 切回 A 时，React 中间态 connected 可能短暂为 false，第 1017 行不触发
    //   3. snapshot apply 完成后 connected 变 true，effect 因 connected dep 再次触发
    //   4. 但 activeTerminalTabIdRef.current === tabId（都是 A），提前 return，
    //      wasConnectedRef.current = true 永远不执行
    //   5. A 已连接，main 不再发 terminal:state，wasConnectedRef 永久卡 false
    //   6. onData 走 !wasConnectedRef 分支吞掉所有输入——"切回来无法输入"
    //
    // 把提升逻辑提到 return 之前：只要 connected 为 true，无论是否切换 tab，
    // 都把 wasConnectedRef 从 false 提升到 true。真正的断开仍由 onTerminalState
    // (line 823) 权威设置，这里只做单向提升，不会反向覆盖。
    if (connected) {
      wasConnectedRef.current = true
    }

    if (activeTerminalTabIdRef.current === tabId) {
      return
    }

    activeTerminalTabIdRef.current = tabId
    const terminal = terminalRef.current
    const host = hostRef.current
    if (!terminal || !host) {
      return
    }

    preserveVisibleBufferRef.current = false
    awaitingCommandCompletionRef.current = false
    pendingPromptResizeRef.current = false
    replaceTerminalWithTranscript(terminal, hydratedBootText)
    lastSyncedSizeRef.current = null

    const { width, height } = host.getBoundingClientRect()
    if (width > 0 && height > 0) {
      void window.fileterm?.resizeTerminal(tabId, terminal.cols, terminal.rows, Math.floor(width), Math.floor(height))
    }
  }, [connected, hydratedBootText, tabId])

  useEffect(() => {
    const applyCurrentProfileFontSize = () => {
      if (profileIdRef.current !== profileId) {
        return
      }

      applyTerminalFontSize(getTerminalFontSize(profileId))
    }

    applyCurrentProfileFontSize()
    return subscribeTerminalFontSizes(applyCurrentProfileFontSize)
  }, [profileId])

  useEffect(() => {
    bootTextRef.current = hydratedBootText
    const terminal = terminalRef.current
    if (
      !terminal ||
      connected ||
      !shouldHydrateTranscript(renderedTranscriptRef.current, hydratedBootText, wasConnectedRef.current)
    ) {
      return
    }

    replaceTerminalWithTranscript(terminal, hydratedBootText)
  }, [connected, hydratedBootText])

  useEffect(() => {
    if (!terminalRef.current) {
      return
    }

    applyTerminalTheme()
  }, [findOpen, findQuery])

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => {
      applyTerminalTheme()
      const terminal = terminalRef.current
      const configuredFontFamily = getConfiguredMonoFontFamily()
      if (terminal && terminal.options.fontFamily !== configuredFontFamily) {
        terminal.options.fontFamily = configuredFontFamily
        terminal.clearTextureAtlas()
        terminal.refresh(0, Math.max(terminal.rows - 1, 0))
      }
    })

    observer.observe(root, {
      attributes: true,
      attributeFilter: ['data-theme', 'style', 'class']
    })

    return () => observer.disconnect()
  }, [findOpen, findQuery])

  useEffect(() => {
    if (!findOpen) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      findInputRef.current?.focus()
      findInputRef.current?.select()
    })

    return () => window.cancelAnimationFrame(frame)
  }, [findOpen])

  useEffect(() => {
    if (!findOpen) {
      return
    }

    if (!findQuery) {
      setFindMiss(false)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return
    }

    const searchAddon = searchAddonRef.current
    if (!searchAddon) {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return
    }

    try {
      const found = searchAddon.findNext(findQuery, buildSearchOptions(true))
      if (!found) {
        setFindMiss(true)
        setFindMatchCount(0)
        setActiveFindIndex(-1)
        clearFindSelection()
      }
    } catch {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
    }
  }, [findCaseSensitive, findOpen, findQuery, findRegex])

  return {
    hostRef,
    terminalScrollController,
    findInputRef,
    hasSelection,
    contextMenu,
    findOpen,
    findQuery,
    findMiss,
    findMatchCount,
    activeFindIndex,
    findCaseSensitive,
    findRegex,
    shortcuts,
    setContextMenu,
    setFindQuery,
    setFindMiss,
    setActiveFindIndex,
    setFindCaseSensitive,
    setFindRegex,
    closeFind,
    searchTerminal,
    runCopy,
    runPaste,
    runFind,
    runSaveSessionLog,
    runClear,
    runSplitPane,
    runClosePane
  }
}
