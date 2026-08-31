import type { Dispatch, SetStateAction } from 'react'
import type { FitAddon } from '@xterm/addon-fit'
import type { SearchAddon } from '@xterm/addon-search'
import type { Terminal, ITheme } from '@xterm/xterm'
import type { TerminalZoomOperation } from '@fileterm/core'
import type { TerminalLogColorizer } from '../app/terminal-log-colorizer'

export type MutableRef<T> = { current: T }
export type TerminalSize = { cols: number; rows: number; width: number; height: number }
export type TerminalHostRect = {
  left: number
  top: number
  right: number
  bottom: number
  width: number
  height: number
}
export type TerminalResizeOptions = {
  force?: boolean
  freezeCols?: boolean
  preserveVisibleBuffer?: boolean
}

export type TerminalLifecycleOptions = {
  isMac: boolean
  isWindowsPty: boolean
  isActive: boolean
  bootText: string
  hostRef: MutableRef<HTMLDivElement | null>
  setTerminalScrollableElement(value: HTMLElement | null): void
  terminalRef: MutableRef<Terminal | null>
  fitAddonRef: MutableRef<FitAddon | null>
  terminalLogColorizerRef: MutableRef<TerminalLogColorizer | null>
  searchAddonRef: MutableRef<SearchAddon | null>
  bootTextRef: MutableRef<string>
  renderedTranscriptRef: MutableRef<string>
  pendingWriteRef: MutableRef<string>
  writeFrameRef: MutableRef<number | null>
  resizeTimerRef: MutableRef<number | null>
  resizeSettleTimerRef: MutableRef<number | null>
  pendingResizeForceRef: MutableRef<boolean>
  pendingResizeFreezeColsRef: MutableRef<boolean>
  isWritingRef: MutableRef<boolean>
  suppressHydratedChunksUntilRef: MutableRef<number>
  preserveVisibleBufferRef: MutableRef<boolean>
  bootedTabs: MutableRef<Set<string>>
  wasConnectedRef: MutableRef<boolean>
  inputSendFailedRef: MutableRef<boolean>
  imeCompositionActiveRef: MutableRef<boolean>
  imeCompositionTextRef: MutableRef<string>
  imeCompositionEndedAtRef: MutableRef<number>
  imeCompositionDataForwardedRef: MutableRef<boolean>
  replayingTranscriptRef: MutableRef<boolean>
  transcriptReplayGenerationRef: MutableRef<number>
  connectedRef: MutableRef<boolean>
  connectingRef: MutableRef<boolean>
  isActiveRef: MutableRef<boolean>
  serialTransferBusyRef: MutableRef<boolean>
  lastSyncedSizeRef: MutableRef<TerminalSize | null>
  lastObservedHostRectRef: MutableRef<TerminalHostRect | null>
  isHorizontalResizeActiveRef: MutableRef<boolean>
  lastTerminalOutputAtRef: MutableRef<number>
  awaitingCommandCompletionRef: MutableRef<boolean>
  pendingPromptResizeRef: MutableRef<boolean>
  profileIdRef: MutableRef<string>
  tabIdRef: MutableRef<string>
  onStatusRef: MutableRef<((message: string | null) => void) | undefined>
  onReconnectRef: MutableRef<(() => void | Promise<void>) | undefined>
  closedMessageRef: MutableRef<string>
  reconnectHintRef: MutableRef<string>
  canClosePaneRef: MutableRef<boolean>
  isReconnectingRef: MutableRef<boolean>
  reconnectHintShownRef: MutableRef<boolean>
  activeTerminalTabIdRef: MutableRef<string | null>
  findOpenRef: MutableRef<boolean>
  terminalZoomLockedRef: MutableRef<boolean>
  setHasSelection: Dispatch<SetStateAction<boolean>>
  setContextMenu: Dispatch<SetStateAction<{ x: number; y: number } | null>>
  setFindMatchCount: Dispatch<SetStateAction<number>>
  setActiveFindIndex: Dispatch<SetStateAction<number>>
  setFindMiss: Dispatch<SetStateAction<boolean>>
  buildTerminalTheme(): ITheme
  applyTerminalFontSize(fontSize: number): boolean
  syncTerminalSize(fitAddon: FitAddon, terminal: Terminal, options?: TerminalResizeOptions): void
  replaceTerminalWithTranscript(terminal: Terminal, transcript: string): void
  shouldHydrateTranscript(currentTranscript: string, nextTranscript: string, connected: boolean): boolean
  formatTerminalChunk(terminal: Terminal | null, value: string): string
  appendRenderedTranscript(chunk: string): void
  scheduleTerminalWrite(text: string): void
  buildExitAlternateScreenSequence(): string
  snapshotTerminalBuffer(terminal: Terminal): string
  clearSearchDecorations(): void
  clearEphemeralHighlight(): void
  runCopy(): void
  runPaste(): Promise<void>
  openFind(): void
  closeFind(): void
  runSplitPane(direction: 'row' | 'column'): void
  runClosePane(): void
  runCloseTab(): void
}

export type TerminalLifecycleRuntime = TerminalLifecycleOptions & {
  terminal: Terminal
  fitAddon: FitAddon
  searchAddon: SearchAddon
  terminalTextarea: HTMLTextAreaElement | null
  terminalLogColorizer: TerminalLogColorizer
  adjustTerminalFontSize(change: number): boolean
  applyTerminalZoom(operation: TerminalZoomOperation, source: 'menu' | 'shortcut' | 'gesture', steps?: number): boolean
  markTerminalFocused(): void
  markTerminalUnderPointer(): void
  clearTerminalUnderPointer(): void
  writeReconnectHint(): void
  writeTerminalInput(data: string): void
  requestReconnect(): boolean
  getTerminalZoomOperation(event: KeyboardEvent): TerminalZoomOperation | null
  isLastFocusedTerminal(): boolean
  isTerminalUnderPointer(): boolean
  clearGlobalTerminalState(): void
  disposeCore(): void
}

export type TerminalLifecycleResizeHandlers = {
  resize(force?: boolean, freezeCols?: boolean, preserveVisibleBuffer?: boolean): void
  scheduleResize(force?: boolean, freezeCols?: boolean, preserveVisibleBuffer?: boolean): void
  scheduleSettledHorizontalResize(): void
  onWindowFocus(): void
  onVisibilityChange(): void
  registerSessionEvents(): () => void
  dispose(): void
}

export type TerminalLifecycleInputHandlers = {
  onTerminalInputCapture(event: Event): void
  onTerminalInputAfterXterm(event: Event): void
  resetInputState(): void
  dispose(): void
}
