import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import type { PaneNode, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { TerminalView } from '../../components/TerminalView'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

interface SplitPaneLayoutProps {
  rootTab: WorkspaceTab
  sessions: Record<string, SessionSnapshot>
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onResizeEnd(panePath: number[], weights: number[]): void
}

/**
 * 分屏布局：递归渲染 PaneNode 树。
 *
 * 每个 leaf 渲染一个 TerminalView（独立 session，不共享 PTY）。
 * split 节点按 direction（row=左右，column=上下）排列子节点，中间有 resizer；
 * 组件由 SessionWorkspace 挂进终端区域，因此文件、命令和侧栏仍是共享工作区。
 */
export function SplitPaneLayout({
  rootTab,
  sessions,
  activePaneTabId,
  onClosePane,
  onSplitPane,
  onActivatePane,
  onResizeEnd
}: SplitPaneLayoutProps) {
  if (!rootTab.paneRoot) {
    return null
  }

  // 只有 >1 个 pane 时，单个 pane 的右键菜单才显示"关闭当前分屏"；
  // 单 pane 时关闭等价于关 tab，走平台关闭键的确认流程。
  const canClosePane = countPaneLeaves(rootTab.paneRoot) > 1

  return (
    <div className="split-pane-root">
      <PaneRenderer
        node={rootTab.paneRoot}
        sessions={sessions}
        rootTabId={rootTab.id}
        panePath={[]}
        activePaneTabId={activePaneTabId}
        onClosePane={onClosePane}
        canClosePane={canClosePane}
        onSplitPane={onSplitPane}
        onActivatePane={onActivatePane}
        onResizeEnd={onResizeEnd}
      />
    </div>
  )
}

function countPaneLeaves(node: PaneNode): number {
  if (node.kind === 'leaf') {
    return 1
  }
  return node.children.reduce((sum, child) => sum + countPaneLeaves(child), 0)
}

interface PaneRendererProps {
  node: PaneNode
  sessions: Record<string, SessionSnapshot>
  rootTabId: string
  panePath: number[]
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  canClosePane: boolean
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onResizeEnd(panePath: number[], weights: number[]): void
}

function PaneRenderer({
  node,
  sessions,
  rootTabId,
  panePath,
  activePaneTabId,
  onClosePane,
  canClosePane,
  onSplitPane,
  onActivatePane,
  onResizeEnd
}: PaneRendererProps) {
  if (node.kind === 'leaf') {
    const session = sessions[node.tabId]
    const isActive = activePaneTabId === node.tabId
    return (
      <div className={`split-pane-leaf ${isActive ? 'split-pane-leaf--active' : ''}`}>
        <div className="split-pane-terminal">
          <TerminalView
            tabId={node.tabId}
            bootText={session?.terminalTranscript ?? ''}
            connected={session?.connected ?? false}
            connecting={session?.connected === false}
            onSplitPane={(direction) => onSplitPane(node.tabId, direction)}
            onClosePane={() => onClosePane(node.tabId)}
            canClosePane={canClosePane}
            onActivate={() => {
              if (!isActive) {
                onActivatePane(node.tabId)
              }
            }}
          />
        </div>
        <CloseButton
          aria-label={t.closeTab}
          className="split-pane-leaf-close"
          onMouseDown={(event) => event.stopPropagation()}
          onClick={() => onClosePane(node.tabId)}
          size="compact"
        />
      </div>
    )
  }

  // split 节点
  const isRow = node.direction === 'row'
  const panes = node.children
  const weights = node.weights.length === panes.length ? node.weights : panes.map(() => 1 / panes.length)

  return (
    <SplitContainer
      isRow={isRow}
      panes={panes}
      initialWeights={weights}
      sessions={sessions}
      rootTabId={rootTabId}
      panePath={panePath}
      activePaneTabId={activePaneTabId}
      onClosePane={onClosePane}
      canClosePane={canClosePane}
      onSplitPane={onSplitPane}
      onActivatePane={onActivatePane}
      onResizeEnd={onResizeEnd}
    />
  )
}

interface SplitContainerProps {
  isRow: boolean
  panes: PaneNode[]
  initialWeights: number[]
  sessions: Record<string, SessionSnapshot>
  rootTabId: string
  panePath: number[]
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  canClosePane: boolean
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onResizeEnd(panePath: number[], weights: number[]): void
}

function SplitContainer({
  isRow,
  panes,
  initialWeights,
  sessions,
  rootTabId,
  panePath,
  activePaneTabId,
  onClosePane,
  canClosePane,
  onSplitPane,
  onActivatePane,
  onResizeEnd
}: SplitContainerProps) {
  const [weights, setWeights] = useState(initialWeights)
  const [dragging, setDragging] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)

  // 当后端 weights 变化时（如新分屏），同步前端
  useEffect(() => {
    if (!dragging) {
      setWeights(initialWeights)
    }
  }, [initialWeights, dragging])

  const handleResizeStart = useCallback(
    (event: React.MouseEvent, resizerIndex: number) => {
      event.preventDefault()
      event.stopPropagation()
      // 只支持两个子节点的拖拽（MVP）
      if (panes.length !== 2 || resizerIndex !== 0) return

      setDragging(true)

      const container = containerRef.current
      if (!container) return

      const rect = container.getBoundingClientRect()
      const startXY = isRow ? event.clientX : event.clientY
      const startWeights = [...weights]

      const onMove = (e: MouseEvent) => {
        const currentXY = isRow ? e.clientX : e.clientY
        const totalSize = isRow ? rect.width : rect.height
        const delta = (currentXY - startXY) / totalSize
        const newFirst = Math.max(0.1, Math.min(0.9, startWeights[0] + delta))
        const newSecond = 1 - newFirst
        setWeights([newFirst, newSecond])
      }

      const onUp = () => {
        setDragging(false)
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
        setWeights((current) => {
          onResizeEnd(panePath, current)
          return current
        })
      }

      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [isRow, weights, panes.length, onResizeEnd]
  )

  // 交替渲染 child 和 resizer
  const items: ReactNode[] = []
  panes.forEach((child, idx) => {
    items.push(
      <div key={`child-${idx}`} className="split-pane-child" style={getChildStyle(isRow, weights, idx, panes.length)}>
        <PaneRenderer
          node={child}
          sessions={sessions}
          rootTabId={rootTabId}
          panePath={[...panePath, idx]}
          activePaneTabId={activePaneTabId}
          onClosePane={onClosePane}
          canClosePane={canClosePane}
          onSplitPane={onSplitPane}
          onActivatePane={onActivatePane}
          onResizeEnd={onResizeEnd}
        />
      </div>
    )
    if (idx < panes.length - 1) {
      items.push(
        <div
          key={`resizer-${idx}`}
          className={`split-pane-resizer ${isRow ? 'split-pane-resizer--row' : 'split-pane-resizer--column'}`}
          onMouseDown={(e) => handleResizeStart(e, idx)}
        />
      )
    }
  })

  return (
    <div
      ref={containerRef}
      className={`split-pane-container ${isRow ? 'split-pane-container--row' : 'split-pane-container--column'}`}
    >
      {items}
    </div>
  )
}

function getChildStyle(isRow: boolean, weights: number[], idx: number, total: number): CSSProperties {
  const weight = weights[idx] ?? 1 / total
  const size = `${weight * 100}%`
  return {
    flex: `1 1 ${size}`,
    minWidth: 0,
    minHeight: 0,
    overflow: 'hidden'
  }
}
