import { useState } from 'react'
import type { WorkspaceTab } from '@termdock/core'
import { tabStatusClass } from '../../app/app-utils'
import type { AppLocale } from '../../i18n'
import { t } from '../../i18n'
import type { ThemeMode } from '../../hooks/useThemeMode'
import { AppIcon } from '../common/AppIcon'
import { ContextMenu } from '../common/ContextMenu'
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
  locale,
  onAddHomeTab,
  onActivateHome,
  onActivateSession,
  onCloseHomeTab,
  onCloseSessionTab,
  onDragEnd,
  onDragEnter,
  onDragStart,
  onOpenCommandManager,
  onOpenConnectionManager,
  onOpenLogsDirectory,
  onOpenTabContext,
  onSetLocale,
  onSetTheme,
  orderedTabs,
  theme
}: {
  activeHomeTabId: string | null
  activeSessionTabId: string | null
  locale: AppLocale
  onAddHomeTab(): void
  onActivateHome(id: string): void
  onActivateSession(id: string): void
  onCloseHomeTab(event: React.MouseEvent<HTMLButtonElement>, id: string): void
  onCloseSessionTab(event: React.MouseEvent<HTMLButtonElement>, id: string): void
  onDragEnd(): void
  onDragEnter(targetKey: string): void
  onDragStart(tabKey: string): void
  onOpenCommandManager(): void
  onOpenConnectionManager(): void
  onOpenLogsDirectory(): void
  onOpenTabContext(event: React.MouseEvent<HTMLDivElement>, target: TabContextTarget): void
  onSetLocale(locale: AppLocale): void
  onSetTheme(theme: ThemeMode): void
  orderedTabs: OrderedTabEntry[]
  theme: ThemeMode
}) {
  const [toolsMenu, setToolsMenu] = useState<{ x: number; y: number } | null>(null)

  return (
    <header className="fs-tabbar">
      <div className="titlebar-brand">
        <strong>{t.appTitle}</strong>
      </div>
      <div className="titlebar-tabarea">
        <button
          aria-label={t.connectionManager}
          className="tabbar-folder-button"
          onClick={onOpenConnectionManager}
          title={t.connectionManager}
          type="button"
        >
          <AppIcon name="connections" size={16} />
        </button>
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
            aria-label={t.settings}
            title={t.settings}
            type="button"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect()
              setToolsMenu({ x: rect.right - 180, y: rect.bottom + 6 })
            }}
          >
            <AppIcon name="menu" />
          </button>
        </div>
        {toolsMenu ? (
          <ContextMenu
            className="tools-menu"
            items={[
              { label: `${t.theme}: ${t.defaultDark}`, disabled: theme === 'default-dark', action: () => onSetTheme('default-dark') },
              { label: `${t.theme}: ${t.defaultLight}`, disabled: theme === 'default-light', action: () => onSetTheme('default-light') },
              { separator: true },
              { label: '简体中文', disabled: locale === 'zhCN', action: () => onSetLocale('zhCN') },
              { label: 'English', disabled: locale === 'enUS', action: () => onSetLocale('enUS') },
              { separator: true },
              { label: t.commandManager, action: onOpenCommandManager },
              { label: t.connectionManager, action: onOpenConnectionManager },
              { label: t.openLogsDirectory, action: onOpenLogsDirectory },
              { label: t.settings, action: () => window.alert(t.notReady) }
            ]}
            onClose={() => setToolsMenu(null)}
            position={toolsMenu}
          />
        ) : null}
      </div>
    </header>
  )
}
