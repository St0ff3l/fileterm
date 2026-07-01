import type { WorkspaceTab } from '@fileterm/core'
import { tabStatusClass } from '../../app/app-utils'
import { t } from '../../i18n'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'

export type OrderedTabEntry =
  | { key: string; kind: 'local'; id: string; title: string; tabKind: 'home' | 'system' }
  | { key: string; kind: 'session'; tab: WorkspaceTab }

export type TabContextTarget =
  | { kind: 'local'; id: string; title: string }
  | { kind: 'session'; id: string; title: string; status: WorkspaceTab['status'] }

export function TabBar({
  activeHomeTabId,
  activeSessionTabId,
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
  onToggleWorkspaceFocus,
  orderedTabs
}: {
  activeHomeTabId: string | null
  activeSessionTabId: string | null
  isWorkspaceFocusMode: boolean
  onAddHomeTab(): void
  onActivateHome(id: string): void
  onActivateSession(id: string): void
  onCloseHomeTab(event: React.MouseEvent<HTMLButtonElement>, id: string): void
  onCloseSessionTab(event: React.MouseEvent<HTMLButtonElement>, id: string): void
  onDragEnd(): void
  onDragEnter(targetKey: string): void
  onDragStart(tabKey: string): void
  onOpenTabContext(event: React.MouseEvent<HTMLDivElement>, target: TabContextTarget): void
  onOpenSettings(): void
  onToggleWorkspaceFocus(): void
  orderedTabs: OrderedTabEntry[]
}) {
  const focusModeLabel = isWorkspaceFocusMode ? t.exitWorkspaceFocusMode : t.enterWorkspaceFocusMode

  return (
      <header className="fs-tabbar">
        <div className="titlebar-brand">
          <div className="window-controls-decorator" aria-hidden="true">
            <span className="dot dot-close" />
            <span className="dot dot-minimize" />
            <span className="dot dot-maximize" />
          </div>
          <strong>{t.appTitle}</strong>
        </div>
        <div className="titlebar-tabarea">
        <div
          className="fs-tabs"
          onWheel={handleHorizontalWheelScroll}
        >
          {orderedTabs.map((entry, index) => (
            entry.kind === 'local' ? (
              <div
                key={entry.key}
                className={`fs-tab ${entry.tabKind === 'home' ? 'home-tab' : 'system-tab'} ${activeHomeTabId === entry.id ? 'active' : ''}`}
                draggable
                onClick={() => onActivateHome(entry.id)}
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
                onDragEnd={onDragEnd}
                onDragEnter={() => onDragEnter(entry.key)}
                onDragOver={(event) => event.preventDefault()}
                onDragStart={() => onDragStart(entry.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    onActivateHome(entry.id)
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <span>{index + 1}</span>
                <strong>{entry.title}</strong>
                <button aria-label={`${t.closeTab} ${entry.title}`} className="tab-close" onClick={(event) => onCloseHomeTab(event, entry.id)} type="button">×</button>
              </div>
            ) : (
              <div
                key={entry.key}
                className={`fs-tab session-tab ${entry.tab.id === activeSessionTabId && !activeHomeTabId ? 'active' : ''}`}
                draggable
                onClick={() => onActivateSession(entry.tab.id)}
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
                onDragEnd={onDragEnd}
                onDragEnter={() => onDragEnter(entry.key)}
                onDragOver={(event) => event.preventDefault()}
                onDragStart={() => onDragStart(entry.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    onActivateSession(entry.tab.id)
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <span>{index + 1}</span>
                <strong>{entry.tab.title}</strong>
                <span className={`tab-dot ${tabStatusClass(entry.tab.status)}`} />
                <button aria-label={`${t.closeTab} ${entry.tab.title}`} className="tab-close" onClick={(event) => onCloseSessionTab(event, entry.tab.id)} type="button">×</button>
              </div>
            )
          ))}
          <button className="add-tab" type="button" onClick={onAddHomeTab}>+</button>
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
            <span className="material-symbols-outlined">{isWorkspaceFocusMode ? 'close_fullscreen' : 'open_in_full'}</span>
          </button>
          <button
            aria-label={t.settings}
            title={t.settings}
            type="button"
            onClick={onOpenSettings}
          >
            <span className="material-symbols-outlined">settings</span>
          </button>
        </div>
      </div>
    </header>
  )
}
