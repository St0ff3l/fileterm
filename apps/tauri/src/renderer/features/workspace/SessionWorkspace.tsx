import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type Dispatch,
  type DragEvent,
  type SetStateAction
} from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplateInput,
  CommandTemplate,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { TerminalView } from '../../components/TerminalView'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { AppIcon } from '../common/AppIcon'
import { FileManager } from '../files/FileManager'
import { TerminalDock } from '../terminal/TerminalDock'
import { t } from '../../i18n'
import { SplitPaneLayout } from './SplitPaneLayout'
import { FILE_PANEL_SNAP_TARGETS, FILE_PANEL_SNAP_TARGET_SELECTORS, type FilePanelSnapTarget } from './file-panel-snap'

const DEFAULT_FILE_PANEL_HEIGHT = 218
const DEFAULT_FILE_PANEL_RATIO = 30
const MAX_FILE_PANEL_RATIO = 70
const MIN_TERMINAL_PANEL_HEIGHT = 120
const FILE_PANEL_SNAP_THRESHOLD = 10

type FilePanelSnapPoint = {
  target: FilePanelSnapTarget
  height: number
}

export function SessionWorkspace({
  activeTab,
  terminalActiveTab,
  splitRootTab,
  splitPaneSessions,
  activePaneTabId,
  onClosePane,
  onCloseTab,
  onSplitPane,
  onActivatePane,
  onSetPaneWeights,
  activeView,
  onActiveViewChange,
  commandPaneWidth,
  onCommandPaneWidthChange,
  activeSession,
  terminalActiveSession,
  filePanelHeight,
  onFilePanelHeightChange,
  filePanelRatio,
  onFilePanelRatioCommit,
  filePanelSnapTarget,
  onFilePanelSnapTargetCommit,
  rememberFilePanelRatio,
  sendTargets,
  terminalDockSendScope,
  terminalDockSelectedTabIds,
  localItems,
  localPath,
  localPanePath,
  isLocalNetworkShare,
  isLocalDirectoryLoading,
  isWorkspaceRefreshing,
  isWorkspaceSwitching,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  commandFolders,
  commandTemplates,
  isBusy,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onExecuteCommand,
  onSendTerminalCommand,
  onSaveTemporaryCommand,
  onUpdateCommand,
  onTerminalDockSendScopeChange,
  onTerminalDockSelectedTabIdsChange,
  onOpenCommandManager,
  onOpenLocalItem,
  onOpenLocalPath,
  onBackToLocalComputer,
  onOpenRemoteItem,
  onOpenRemotePath,
  onPasteIntoPane,
  onRequestChangePermissions,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onRequestQuickDelete,
  onRequestRename,
  onToggleFollowShellCwd,
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  isRemoteDirectoryLoading,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDownloadLocalNetworkFiles,
  onDropUpload,
  isWorkspaceFocusMode,
  isActive = true
}: {
  activeTab: WorkspaceTab
  terminalActiveTab: WorkspaceTab
  splitRootTab?: WorkspaceTab
  splitPaneSessions: Record<string, SessionSnapshot>
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  onCloseTab(): void
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onSetPaneWeights(panePath: number[], weights: number[]): void
  activeView: 'file' | 'command' | 'tunnel'
  onActiveViewChange(view: 'file' | 'command' | 'tunnel'): void
  commandPaneWidth: number
  onCommandPaneWidthChange(width: number): void
  activeSession: SessionSnapshot
  terminalActiveSession: SessionSnapshot
  filePanelHeight: number
  onFilePanelHeightChange: Dispatch<SetStateAction<number>>
  filePanelRatio: number
  onFilePanelRatioCommit(ratio: number): void
  filePanelSnapTarget: FilePanelSnapTarget | null
  onFilePanelSnapTargetCommit(target: FilePanelSnapTarget | null): void
  rememberFilePanelRatio: boolean
  sendTargets: SessionSendTarget[]
  terminalDockSendScope: SendScope
  terminalDockSelectedTabIds: string[]
  localItems: LocalFileItem[]
  localPath: string
  localPanePath: string
  isLocalNetworkShare: boolean
  isLocalDirectoryLoading: boolean
  isWorkspaceRefreshing: boolean
  isWorkspaceSwitching: boolean
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onExecuteCommand(
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): void
  onSendTerminalCommand(
    command: string,
    options?: CommandExecutionOptions,
    scope?: SendScope,
    selectedTabIds?: string[]
  ): Promise<void>
  onSaveTemporaryCommand(command: string, appendCarriageReturn: boolean): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
  onTerminalDockSendScopeChange(scope: SendScope, rememberSelection: boolean): void
  onTerminalDockSelectedTabIdsChange(tabIds: string[], rememberSelection: boolean): void
  onOpenCommandManager(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onBackToLocalComputer(): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
  onRequestChangePermissions(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onRequestDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestNewFile(pane: 'local' | 'remote', directoryPath: string): void
  onRequestNewFolder(pane: 'local' | 'remote', directoryPath: string): void
  onRequestQuickDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestRename(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onToggleFollowShellCwd(): void
  onToggleRemoteFileAccessMode(): void
  remoteFileAccessMode: 'user' | 'root'
  isRemoteDirectoryLoading: boolean
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDownloadLocalNetworkFiles(items: LocalFileItem[]): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  isWorkspaceFocusMode: boolean
  isActive?: boolean
}) {
  const isFileOnly = activeTab.layout === 'file-only'
  const isTerminalOnly = activeTab.layout === 'terminal-only'
  const setFilePanelHeight = onFilePanelHeightChange
  const [isFilePanelCollapsed, setIsFilePanelCollapsed] = useState(filePanelRatio <= 0 || isWorkspaceFocusMode)
  const [isFilePanelDragging, setIsFilePanelDragging] = useState(false)
  const workspaceRef = useRef<HTMLElement | null>(null)
  const isResizingFilePanel = useRef(false)
  const dragStateRef = useRef<{
    bottom: number
    height: number
    snapTargets: FilePanelSnapPoint[]
    latestSnapTarget: FilePanelSnapTarget | null
    latestHeight: number
  } | null>(null)
  const layoutFrameRef = useRef<number | null>(null)
  const filePanelSnapTargetRef = useRef<FilePanelSnapTarget | null>(null)
  const alignmentInitializedTabRef = useRef<string | null>(null)
  const lastExpandedFilePanelHeight = useRef(filePanelHeight)
  const appliedWorkspaceFocusMode = useRef<boolean | null>(null)
  const isFilePanelEffectivelyCollapsed = isFilePanelCollapsed && !isFileOnly
  const effectiveFilePanelHeight = isFilePanelEffectivelyCollapsed ? 0 : filePanelHeight
  const clampFilePanelHeight = (workspaceHeight: number, nextHeight: number) => {
    const minHeight = 25 // Allow it to shrink to just the tabs row height
    const maxHeight = Math.max(
      minHeight,
      Math.min((workspaceHeight * MAX_FILE_PANEL_RATIO) / 100, workspaceHeight - MIN_TERMINAL_PANEL_HEIGHT)
    )
    return Math.min(maxHeight, Math.max(minHeight, nextHeight))
  }

  const clampFilePanelRatio = (nextRatio: number) =>
    Math.max(0, Math.min(MAX_FILE_PANEL_RATIO, Number.isFinite(nextRatio) ? nextRatio : DEFAULT_FILE_PANEL_RATIO))

  const filePanelHeightFromRatio = (workspaceHeight: number, ratio: number) => {
    const normalizedRatio = clampFilePanelRatio(ratio)
    return normalizedRatio <= 0 ? 0 : clampFilePanelHeight(workspaceHeight, (workspaceHeight * normalizedRatio) / 100)
  }

  const filePanelRatioFromHeight = (workspaceHeight: number, height: number) => {
    if (height <= 0 || workspaceHeight <= 0) return 0
    return clampFilePanelRatio(Number(((height / workspaceHeight) * 100).toFixed(2)))
  }

  const getFilePanelSnapTargetElement = (target: FilePanelSnapTarget) =>
    document.querySelector<HTMLElement>(`.fs-sidebar:not(.is-collapsed) ${FILE_PANEL_SNAP_TARGET_SELECTORS[target]}`) ??
    null

  const getFilePanelSnapHeight = (target: FilePanelSnapTarget, workspaceRect: DOMRect) => {
    const targetRect = getFilePanelSnapTargetElement(target)?.getBoundingClientRect()
    if (!targetRect || targetRect.height <= 0) {
      return null
    }

    const nextHeight = workspaceRect.bottom - targetRect.top
    const clampedHeight = clampFilePanelHeight(workspaceRect.height, nextHeight)
    return Math.abs(nextHeight - clampedHeight) <= 0.5 ? clampedHeight : null
  }

  const getFilePanelSnapPoints = (workspaceRect: DOMRect): FilePanelSnapPoint[] =>
    FILE_PANEL_SNAP_TARGETS.flatMap((target) => {
      const height = getFilePanelSnapHeight(target, workspaceRect)
      return height === null ? [] : [{ target, height }]
    })

  const syncFilePanelHeight = (mode: 'align' | 'clamp' = 'clamp') => {
    if (!isActive || isFileOnly || isFilePanelCollapsed || !workspaceRef.current || isResizingFilePanel.current) {
      return
    }

    const workspaceRect = workspaceRef.current.getBoundingClientRect()
    if (workspaceRect.height <= 0) {
      return
    }

    if (mode === 'align' && filePanelSnapTargetRef.current) {
      const snapHeight = getFilePanelSnapHeight(filePanelSnapTargetRef.current, workspaceRect)
      if (snapHeight !== null) {
        setFilePanelHeight((prev) => (prev === snapHeight ? prev : snapHeight))
        return
      }
    }

    const nextHeight = filePanelHeightFromRatio(workspaceRect.height, filePanelRatio)
    setFilePanelHeight((prev) => (prev === nextHeight ? prev : nextHeight))
  }

  useEffect(() => {
    if (!isFilePanelCollapsed && filePanelHeight > 0) {
      lastExpandedFilePanelHeight.current = filePanelHeight
    }
  }, [filePanelHeight, isFilePanelCollapsed])

  useEffect(() => {
    if (!isActive || isFileOnly) {
      return
    }
    if (appliedWorkspaceFocusMode.current === isWorkspaceFocusMode) {
      return
    }
    appliedWorkspaceFocusMode.current = isWorkspaceFocusMode

    if (isWorkspaceFocusMode) {
      if (!isFilePanelCollapsed && filePanelHeight > 0) {
        lastExpandedFilePanelHeight.current = filePanelHeight
      }
      isResizingFilePanel.current = false
      setIsFilePanelDragging(false)
      dragStateRef.current = null
      setIsFilePanelCollapsed(true)
      return
    }

    setFilePanelHeight((prev) => (prev > 0 ? prev : lastExpandedFilePanelHeight.current || DEFAULT_FILE_PANEL_HEIGHT))
    setIsFilePanelCollapsed(false)
  }, [isActive, isFileOnly, isWorkspaceFocusMode])

  useEffect(() => {
    isResizingFilePanel.current = false
    dragStateRef.current = null
    filePanelSnapTargetRef.current = filePanelSnapTarget
    setIsFilePanelDragging(false)
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
  }, [activeTab.id, filePanelSnapTarget])

  useEffect(() => {
    if (!isActive || isFileOnly) {
      return
    }

    let dragFrame: number | null = null

    const stopFilePanelDragging = () => {
      const finalDragState = dragStateRef.current
      if (rememberFilePanelRatio && finalDragState) {
        if (workspaceRef.current) {
          onFilePanelRatioCommit(filePanelRatioFromHeight(finalDragState.height, finalDragState.latestHeight))
        }
        onFilePanelSnapTargetCommit(finalDragState.latestSnapTarget)
      }
      isResizingFilePanel.current = false
      dragStateRef.current = null
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
        dragFrame = null
      }
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      setIsFilePanelDragging(false)
    }

    const onMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFilePanel.current || !dragStateRef.current) {
        return
      }

      const { bottom, height, snapTargets } = dragStateRef.current
      let nextHeight = bottom - event.clientY

      const nearestSnapTarget = snapTargets.reduce<{ point: FilePanelSnapPoint; distance: number } | null>(
        (nearest, point) => {
          const distance = Math.abs(nextHeight - point.height)
          return !nearest || distance < nearest.distance ? { point, distance } : nearest
        },
        null
      )
      const snappedTarget =
        nearestSnapTarget && nearestSnapTarget.distance <= FILE_PANEL_SNAP_THRESHOLD ? nearestSnapTarget.point : null
      if (snappedTarget) {
        nextHeight = snappedTarget.height
      }
      filePanelSnapTargetRef.current = snappedTarget?.target ?? null
      dragStateRef.current.latestSnapTarget = snappedTarget?.target ?? null
      dragStateRef.current.latestHeight = clampFilePanelHeight(height, nextHeight)

      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }

      dragFrame = window.requestAnimationFrame(() => {
        setFilePanelHeight((prev) => {
          const clamped = clampFilePanelHeight(height, nextHeight)
          return prev === clamped ? prev : clamped
        })
      })
    }

    const onMouseUp = () => {
      stopFilePanelDragging()
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    window.addEventListener('blur', onMouseUp)
    document.addEventListener('mouseup', onMouseUp)

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      window.removeEventListener('blur', onMouseUp)
      document.removeEventListener('mouseup', onMouseUp)
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      setIsFilePanelDragging(false)
    }
  }, [
    isActive,
    isFileOnly,
    onFilePanelSnapTargetCommit,
    onFilePanelRatioCommit,
    rememberFilePanelRatio,
    setFilePanelHeight
  ])

  useEffect(() => {
    if (!isActive || isFileOnly) {
      return
    }

    const ratioKey = `${activeTab.id}:${filePanelRatio}:${filePanelSnapTarget ?? 'none'}:${rememberFilePanelRatio}`
    if (alignmentInitializedTabRef.current === ratioKey) {
      return
    }

    let checkTimer: number | null = null
    let retries = 0

    const applyStoredRatio = () => {
      if (!workspaceRef.current) return false

      const workspaceRect = workspaceRef.current.getBoundingClientRect()
      if (workspaceRect.height <= 0) return false

      if (filePanelRatio <= 0) {
        setIsFilePanelCollapsed(true)
        setFilePanelHeight(0)
        filePanelSnapTargetRef.current = filePanelSnapTarget
        alignmentInitializedTabRef.current = ratioKey
        return true
      }

      const snapHeight = filePanelSnapTarget ? getFilePanelSnapHeight(filePanelSnapTarget, workspaceRect) : null
      const nextHeight = snapHeight ?? filePanelHeightFromRatio(workspaceRect.height, filePanelRatio)

      setIsFilePanelCollapsed(filePanelRatio <= 0)
      setFilePanelHeight((prev) => (prev === nextHeight ? prev : nextHeight))
      if (nextHeight > 0) lastExpandedFilePanelHeight.current = nextHeight
      filePanelSnapTargetRef.current = filePanelSnapTarget
      alignmentInitializedTabRef.current = ratioKey
      return true
    }

    const runCheck = () => {
      const success = applyStoredRatio()
      if (!success && retries < 10) {
        retries++
        checkTimer = window.setTimeout(runCheck, 60)
      } else if (!success) {
        alignmentInitializedTabRef.current = ratioKey
      }
    }
    runCheck()

    return () => {
      if (checkTimer !== null) {
        window.clearTimeout(checkTimer)
      }
    }
  }, [
    activeTab.id,
    filePanelRatio,
    filePanelSnapTarget,
    isActive,
    isFileOnly,
    rememberFilePanelRatio,
    setFilePanelHeight
  ])

  useEffect(() => {
    if (!isActive || isFileOnly || isFilePanelCollapsed || !workspaceRef.current) {
      return
    }

    let themeTimer: number | null = null

    const syncAfterLayout = () => {
      if (layoutFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutFrameRef.current)
      }

      layoutFrameRef.current = window.requestAnimationFrame(() => {
        layoutFrameRef.current = null
        syncFilePanelHeight(filePanelSnapTargetRef.current ? 'align' : 'clamp')
      })
    }

    const resizeObserver = new ResizeObserver(() => {
      syncAfterLayout()
    })
    resizeObserver.observe(workspaceRef.current)

    // The snap targets are in the sibling system sidebar rather than inside
    // the workspace. Observe both sides of the alignment so a window resize
    // or a sidebar content reflow keeps the horizontal boundaries together.
    const layoutRoot = document.querySelector<HTMLElement>('.fs-shell') ?? document.body
    const sidebar = document.querySelector<HTMLElement>('.fs-sidebar:not(.is-collapsed)')
    const sidebarCard = sidebar?.querySelector<HTMLElement>('.sys-card')
    let observedSnapTargets: HTMLElement[] = []
    const observeSnapTargets = () => {
      const nextSnapTargets = FILE_PANEL_SNAP_TARGETS.flatMap((target) => {
        const element = getFilePanelSnapTargetElement(target)
        return element ? [element] : []
      })
      observedSnapTargets.forEach((element) => {
        if (!nextSnapTargets.includes(element)) {
          resizeObserver.unobserve(element)
        }
      })
      nextSnapTargets.forEach((element) => {
        if (!observedSnapTargets.includes(element)) {
          resizeObserver.observe(element)
        }
      })
      observedSnapTargets = nextSnapTargets
    }
    if (sidebar) resizeObserver.observe(sidebar)
    if (sidebarCard) resizeObserver.observe(sidebarCard)
    observeSnapTargets()

    // The sidebar can replace its metric card while a session reconnects or
    // while the compact layout is rebuilt. Keep observing the current header
    // instead of retaining a detached DOM node from the first render.
    const layoutMutationObserver = new MutationObserver(() => {
      observeSnapTargets()
      syncAfterLayout()
    })
    layoutMutationObserver.observe(layoutRoot, {
      attributes: true,
      attributeFilter: ['class', 'style'],
      childList: true,
      subtree: true
    })

    const themeObserver = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === 'attributes' && ['data-theme', 'class', 'style'].includes(mutation.attributeName || '')) {
          syncAfterLayout()
          if (themeTimer !== null) {
            window.clearTimeout(themeTimer)
          }
          themeTimer = window.setTimeout(syncAfterLayout, 60)
          break
        }
      }
    })

    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'class', 'style']
    })

    window.addEventListener('resize', syncAfterLayout)

    return () => {
      if (layoutFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutFrameRef.current)
        layoutFrameRef.current = null
      }
      if (themeTimer !== null) {
        window.clearTimeout(themeTimer)
        themeTimer = null
      }
      resizeObserver.disconnect()
      layoutMutationObserver.disconnect()
      themeObserver.disconnect()
      window.removeEventListener('resize', syncAfterLayout)
    }
  }, [isActive, isFileOnly, isFilePanelCollapsed, filePanelSnapTarget, filePanelRatio, setFilePanelHeight])

  const handleToggleFilePanelCollapsed = () => {
    if (isFilePanelCollapsed) {
      const nextHeight = lastExpandedFilePanelHeight.current || DEFAULT_FILE_PANEL_HEIGHT
      setFilePanelHeight((prev) => (prev > 0 ? prev : nextHeight))
      setIsFilePanelCollapsed(false)
      if (rememberFilePanelRatio && workspaceRef.current) {
        const workspaceHeight = workspaceRef.current.getBoundingClientRect().height
        onFilePanelRatioCommit(filePanelRatioFromHeight(workspaceHeight, nextHeight))
        onFilePanelSnapTargetCommit(filePanelSnapTargetRef.current)
      }
      return
    }

    if (filePanelHeight > 0) {
      lastExpandedFilePanelHeight.current = filePanelHeight
    }
    isResizingFilePanel.current = false
    dragStateRef.current = null
    setIsFilePanelCollapsed(true)
    if (rememberFilePanelRatio) {
      onFilePanelRatioCommit(0)
      onFilePanelSnapTargetCommit(filePanelSnapTargetRef.current)
    }
  }

  const reconnectMode = terminalActiveSession.reconnectMode ?? 'none'
  const reconnectOnEnter =
    reconnectMode === 'enter' || reconnectMode === 'auto'
      ? async () => {
          await window.fileterm?.reconnectTab(terminalActiveTab.id)
        }
      : undefined
  const canSplitTerminal = terminalActiveTab.sessionType === 'ssh'

  return (
    <section
      className={`session-workspace ${isFileOnly ? 'file-only' : ''} ${isTerminalOnly ? 'terminal-only' : ''} ${isFilePanelEffectivelyCollapsed ? 'file-panel-collapsed' : ''} ${isFilePanelDragging ? 'is-file-panel-dragging' : ''}`}
      ref={workspaceRef}
      style={{ '--file-panel-height': `${effectiveFilePanelHeight}px` } as CSSProperties}
    >
      {!isFileOnly ? (
        <div className={`terminal-area has-terminal-dock ${splitRootTab?.paneRoot ? 'is-terminal-split' : ''}`}>
          {splitRootTab?.paneRoot ? (
            <SplitPaneLayout
              rootTab={splitRootTab}
              sessions={splitPaneSessions}
              isWorkspaceActive={isActive}
              activePaneTabId={activePaneTabId}
              onClosePane={onClosePane}
              onCloseTab={onCloseTab}
              onSplitPane={onSplitPane}
              onActivatePane={onActivatePane}
              onResizeEnd={onSetPaneWeights}
            />
          ) : (
            <TerminalView
              // Keep one xterm parser/write queue per session. Reusing the
              // same instance across tabs makes a fast TUI's pending writes
              // race with transcript replay and can leave a blank canvas.
              key={terminalActiveTab.id}
              profileId={terminalActiveTab.profileId}
              tabId={terminalActiveTab.id}
              bootText={terminalActiveSession.terminalTranscript ?? ''}
              sessionType={terminalActiveTab.sessionType}
              isActive={isActive}
              connected={terminalActiveSession.connected === true}
              connecting={terminalActiveTab.status === 'connecting'}
              onReconnect={reconnectOnEnter}
              onSplitPane={canSplitTerminal ? (direction) => onSplitPane(terminalActiveTab.id, direction) : undefined}
              onCloseTab={onCloseTab}
            />
          )}
          {isActive ? (
            <TerminalDock
              activeTab={terminalActiveTab}
              connected={terminalActiveSession.connected === true}
              selectedTabIds={terminalDockSelectedTabIds}
              sendScope={terminalDockSendScope}
              sendTargets={sendTargets}
              onSelectedTabIdsChange={onTerminalDockSelectedTabIdsChange}
              onSendCommand={onSendTerminalCommand}
              onSendScopeChange={onTerminalDockSendScopeChange}
              onReconnect={reconnectOnEnter}
            />
          ) : null}
          {!isFileOnly && !isTerminalOnly ? (
            <button
              aria-label={isFilePanelCollapsed ? t.terminalDockShowFilePanel : t.terminalDockHideFilePanel}
              aria-pressed={isFilePanelCollapsed}
              className={`file-panel-drawer-toggle ${isFilePanelCollapsed ? 'is-collapsed' : ''}`}
              title={isFilePanelCollapsed ? t.terminalDockShowFilePanel : t.terminalDockHideFilePanel}
              type="button"
              onClick={handleToggleFilePanelCollapsed}
            >
              <AppIcon name={isFilePanelCollapsed ? 'chevron-up' : 'chevron-down'} size={15} />
            </button>
          ) : null}
        </div>
      ) : null}
      {!isFileOnly && !isTerminalOnly && !isFilePanelCollapsed ? (
        <div
          className="session-split-resizer"
          onMouseDown={(event) => {
            event.preventDefault()
            window.getSelection()?.removeAllRanges()
            isResizingFilePanel.current = true
            setIsFilePanelDragging(true)

            if (workspaceRef.current) {
              const rect = workspaceRef.current.getBoundingClientRect()
              dragStateRef.current = {
                bottom: rect.bottom,
                height: rect.height,
                snapTargets: getFilePanelSnapPoints(rect),
                latestSnapTarget: filePanelSnapTarget,
                latestHeight: filePanelHeight
              }
            }

            document.body.style.cursor = 'row-resize'
            document.body.style.userSelect = 'none'
          }}
          aria-label={t.resizeTerminalPanel}
          aria-orientation="horizontal"
          role="separator"
        />
      ) : null}
      {!isTerminalOnly ? (
        <div className="session-bottom-panel">
          {isActive ? (
            <FileManager
              activeSession={activeSession}
              activeTab={activeTab}
              activeView={activeView}
              onActiveViewChange={onActiveViewChange}
              commandPaneWidth={commandPaneWidth}
              onCommandPaneWidthChange={onCommandPaneWidthChange}
              sendTargets={sendTargets}
              commandFolders={commandFolders}
              commandTemplates={commandTemplates}
              isBusy={isBusy}
              localItems={localItems}
              localPath={localPath}
              localPanePath={localPanePath}
              isLocalNetworkShare={isLocalNetworkShare}
              isLocalDirectoryLoading={isLocalDirectoryLoading}
              isWorkspaceRefreshing={isWorkspaceRefreshing}
              isWorkspaceSwitching={isWorkspaceSwitching}
              canPasteToLocal={canPasteToLocal}
              canPasteToRemote={canPasteToRemote}
              clipboardStatusText={clipboardStatusText}
              localCutPaths={localCutPaths}
              remoteCutPaths={remoteCutPaths}
              onCopyItems={onCopyItems}
              onCutItems={onCutItems}
              onClearCutState={onClearCutState}
              onExecuteCommand={onExecuteCommand}
              onSendTerminalCommand={onSendTerminalCommand}
              onSaveTemporaryCommand={onSaveTemporaryCommand}
              onUpdateCommand={onUpdateCommand}
              onOpenCommandManager={onOpenCommandManager}
              onOpenLocalItem={onOpenLocalItem}
              onOpenLocalPath={onOpenLocalPath}
              onBackToLocalComputer={onBackToLocalComputer}
              onOpenRemoteItem={onOpenRemoteItem}
              onOpenRemotePath={onOpenRemotePath}
              onPasteIntoPane={onPasteIntoPane}
              onRequestChangePermissions={onRequestChangePermissions}
              onRequestDelete={onRequestDelete}
              onRequestNewFile={onRequestNewFile}
              onRequestNewFolder={onRequestNewFolder}
              onRequestQuickDelete={onRequestQuickDelete}
              onRequestRename={onRequestRename}
              onToggleFollowShellCwd={onToggleFollowShellCwd}
              onToggleRemoteFileAccessMode={onToggleRemoteFileAccessMode}
              remoteFileAccessMode={remoteFileAccessMode}
              isRemoteDirectoryLoading={isRemoteDirectoryLoading}
              onRefresh={onRefresh}
              onUploadFiles={onUploadFiles}
              onChooseUploadFiles={onChooseUploadFiles}
              onDownloadFiles={onDownloadFiles}
              onDownloadLocalNetworkFiles={onDownloadLocalNetworkFiles}
              onDropUpload={onDropUpload}
            />
          ) : null}
        </div>
      ) : null}
      <div className="terminal-right-frame" aria-hidden="true" />
    </section>
  )
}
