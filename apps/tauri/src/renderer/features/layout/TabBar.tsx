import { useRef, type MouseEvent, type ReactNode } from 'react'
import type { WorkspaceTab } from '@fileterm/core'
import { tabStatusClass } from '../../app/app-utils'
import { t } from '../../i18n'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'
import { CloseButton } from '../common/CloseButton'
import { AppIcon } from '../common/AppIcon'
import { usePointerSortFallback } from '../../hooks/usePointerSortFallback'

export type OrderedTabEntry =
  | { key: string; kind: 'local'; id: string; title: string; tabKind: 'home' | 'system' }
  | { key: string; kind: 'session'; tab: WorkspaceTab }

export type TabContextTarget =
  | { kind: 'local'; id: string; title: string }
  | { kind: 'session'; id: string; title: string; status: WorkspaceTab['status'] }

export interface TabBarProps {
  activeHomeTabId: string | null
  activeSessionTabId: string | null
  homeBrandContent?: ReactNode
  isWorkspaceFocusMode: boolean
  onAddHomeTab(): void
  onActivateHome(id: string): void
  onActivateSession(id: string): void
  onCloseHomeTab(event: MouseEvent<HTMLButtonElement>, id: string): void
  onCloseSessionTab(event: MouseEvent<HTMLButtonElement>, id: string): void
  onDragEnd(): void
  onDragEnter(targetKey: string): void
  onDragStart(tabKey: string): void
  onOpenTabContext(event: MouseEvent<HTMLDivElement>, target: TabContextTarget): void
  onOpenSettings(): void
  onToggleWindowMaximize(): void
  onToggleWorkspaceFocus(): void
  orderedTabs: OrderedTabEntry[]
}

export function TabBar({
  activeHomeTabId,
  activeSessionTabId,
  homeBrandContent,
  isWorkspaceFocusMode,
  onAddHomeTab,
  onActivateHome,
  onActivateSession,
  onCloseHomeTab,
  onCloseSessionTab,
  onDragEnd,
  onDragEnter,
  onDragStart,
  onOpenTabContext,
  onOpenSettings,
  onToggleWindowMaximize,
  onToggleWorkspaceFocus,
  orderedTabs
}: TabBarProps) {
  const focusModeLabel = isWorkspaceFocusMode ? t.exitWorkspaceFocusMode : t.enterWorkspaceFocusMode
  const suppressTabClickRef = useRef(false)
  const releaseSuppressedClick = () => {
    window.setTimeout(() => {
      suppressTabClickRef.current = false
    }, 0)
  }
  const startPointerSort = usePointerSortFallback<string>({
    onStart: (tabKey) => {
      suppressTabClickRef.current = true
      onDragStart(tabKey)
    },
    onTarget: (_source, target) => onDragEnter(target.id),
    onDrop: (_source, target) => {
      if (target) {
        onDragEnter(target.id)
      }
      onDragEnd()
      releaseSuppressedClick()
    },
    onCancel: () => {
      onDragEnd()
      releaseSuppressedClick()
    }
  })

  return (
    <header
      className="fs-tabbar"
      data-tauri-drag-region="deep"
      onDoubleClick={(event) => {
        const target = event.target instanceof Element ? event.target : null
        if (target?.closest('.fs-tab, .add-tab, .window-tools, .window-controls-decorator')) {
          return
        }
        onToggleWindowMaximize()
      }}
    >
      <div
        className={`titlebar-brand ${homeBrandContent ? 'has-home-brand-content' : ''}`}
        data-tauri-drag-region="deep"
      >
        <div className="window-controls-decorator" aria-hidden="true">
          <span className="dot dot-close" />
          <span className="dot dot-minimize" />
          <span className="dot dot-maximize" />
        </div>
        {homeBrandContent ? (
          <div className="titlebar-home-brand" aria-hidden="true">
            {homeBrandContent}
          </div>
        ) : (
          <strong>{t.appTitle}</strong>
        )}
      </div>
      <div className="titlebar-tabarea">
        <div className="fs-tabs" onWheel={handleHorizontalWheelScroll}>
          {orderedTabs.map((entry, index) =>
            entry.kind === 'local' ? (
              <div
                key={entry.key}
                className={`fs-tab ${entry.tabKind === 'home' ? 'home-tab' : 'system-tab'} ${activeHomeTabId === entry.id ? 'active' : ''}`}
                data-fileterm-sort-id={entry.key}
                data-fileterm-sort-kind="tab"
                onClick={() => {
                  if (!suppressTabClickRef.current) onActivateHome(entry.id)
                }}
                onContextMenu={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  onActivateHome(entry.id)
                  onOpenTabContext(event, {
                    kind: 'local',
                    id: entry.id,
                    title: entry.title
                  })
                }}
                onPointerDown={(event) => startPointerSort(event, entry.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    onActivateHome(entry.id)
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <span>{index + 1}</span>
                <strong>{entry.title}</strong>
                <CloseButton
                  aria-label={`${t.closeTab} ${entry.title}`}
                  onClick={(event) => onCloseHomeTab(event, entry.id)}
                  size="tab"
                />
              </div>
            ) : (
              <div
                key={entry.key}
                className={`fs-tab session-tab ${entry.tab.id === activeSessionTabId && !activeHomeTabId ? 'active' : ''}`}
                data-fileterm-sort-id={entry.key}
                data-fileterm-sort-kind="tab"
                onClick={() => {
                  if (!suppressTabClickRef.current) onActivateSession(entry.tab.id)
                }}
                onContextMenu={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  onActivateSession(entry.tab.id)
                  onOpenTabContext(event, {
                    kind: 'session',
                    id: entry.tab.id,
                    title: entry.tab.title,
                    status: entry.tab.status
                  })
                }}
                onPointerDown={(event) => startPointerSort(event, entry.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    onActivateSession(entry.tab.id)
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <span>{index + 1}</span>
                <strong>{entry.tab.title}</strong>
                <span className={`tab-dot ${tabStatusClass(entry.tab.status)}`} />
                <CloseButton
                  aria-label={`${t.closeTab} ${entry.tab.title}`}
                  onClick={(event) => onCloseSessionTab(event, entry.tab.id)}
                  size="tab"
                />
              </div>
            )
          )}
          <button aria-label={t.newTab} className="add-tab" type="button" onClick={onAddHomeTab}>
            <AppIcon name="plus" size={16} />
          </button>
        </div>
        <div className="window-tools">
          <button
            aria-label={focusModeLabel}
            aria-pressed={isWorkspaceFocusMode}
            className={`workspace-focus-toggle ${isWorkspaceFocusMode ? 'is-active' : ''}`}
            title={focusModeLabel}
            type="button"
            onClick={onToggleWorkspaceFocus}
          >
            <span className="material-symbols-outlined">
              {isWorkspaceFocusMode ? 'close_fullscreen' : 'open_in_full'}
            </span>
          </button>
          <button aria-label={t.settings} title={t.settings} type="button" onClick={onOpenSettings}>
            <span className="material-symbols-outlined">settings</span>
          </button>
        </div>
      </div>
    </header>
  )
}
