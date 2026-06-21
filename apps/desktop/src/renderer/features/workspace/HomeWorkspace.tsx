import type { ConnectionProfile, ConnectionFolder, CommandFolder, CommandTemplate } from '@termdock/core'
import { useState } from 'react'
import { t } from '../../i18n'
import { OverviewPage } from './OverviewPage'
import { QuickLinksPage } from './QuickLinksPage'
import { ConnectionManagerModal } from '../connections/ConnectionManagerModal'
import { CommandManagerModal } from '../commands/CommandManagerModal'
import { SettingsModal } from '../settings/SettingsModal'
import { TabBar } from '../layout/TabBar'

export function HomeWorkspace({
  profiles,
  folders = [],
  commandFolders = [],
  commandTemplates = [],
  theme,
  locale,
  onOpen,
  onCreateConnection,
  onEditConnection,
  onDeleteConnection,
  onCreateConnectionFolder,
  onDeleteConnectionFolder,
  onUpdateConnectionFolder,
  onUpdateConnectionOrder,
  onCreateCommand,
  onUpdateCommand,
  onDeleteCommand,
  onCreateCommandFolder,
  onDeleteCommandFolder,
  onUpdateCommandFolder,
  onUpdateCommandOrder,
  onSetTheme,
  onSetLocale,
  onOpenLogsDirectory,
  tabBarProps
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  commandFolders?: CommandFolder[]
  commandTemplates?: CommandTemplate[]
  theme: 'default-dark' | 'default-light'
  locale: 'zhCN' | 'enUS'
  onOpen(profileId: string): void
  onCreateConnection(): void
  onEditConnection(profile: ConnectionProfile): void
  onDeleteConnection(profileId: string): void
  onCreateConnectionFolder(name: string): void
  onDeleteConnectionFolder(folderId: string): void
  onUpdateConnectionFolder(folderId: string, updates: Partial<ConnectionFolder>): void
  onUpdateConnectionOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onCreateCommand(input: any): void
  onUpdateCommand(commandId: string, input: any): void
  onDeleteCommand(commandId: string): void
  onCreateCommandFolder(name: string): void
  onDeleteCommandFolder(folderId: string): void
  onUpdateCommandFolder(folderId: string, updates: Partial<CommandFolder>): void
  onUpdateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onSetTheme(value: 'default-dark' | 'default-light'): void
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  tabBarProps: any
}) {
  const [activeTab, setActiveTab] = useState<'overview' | 'quick-links' | 'command-manager' | 'connection-manager' | 'settings'>('overview')
  const [navDirection, setNavDirection] = useState<'down' | 'up'>('down')
  const [activeConnectionFolderName, setActiveConnectionFolderName] = useState('')
  const [activeCommandFolderName, setActiveCommandFolderName] = useState('')

  // 侧栏页签的纵向顺序,用于判断切换方向(目标更靠下=向下飞入,更靠上=向上飞入)
  const tabOrder: Record<string, number> = {
    overview: 0,
    'quick-links': 1,
    'command-manager': 2,
    'connection-manager': 3,
    settings: 4
  }
  const selectTab = (tab: typeof activeTab) => {
    if (tab === activeTab) return
    setNavDirection((tabOrder[tab] ?? 0) >= (tabOrder[activeTab] ?? 0) ? 'down' : 'up')
    setActiveTab(tab)
  }

  const desktopApi = window.termdock
  const isWindows = desktopApi?.platform === 'win32'

  const handleOpenNewConnection = () => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('create')
    }
  }

  const handleOpenDocs = () => {
    if (desktopApi) {
      void desktopApi.openExternalUrl('https://github.com/St0ff3l/termdock')
    }
  }

  return (
    <section className="home-workspace">
      {/* SideNavBar Component */}
      <aside className="home-sidebar">
        <div className="sidebar-drag-handle" />
        {/* macOS style Window Controls */}
        <div className="window-controls-decorator">
          <div className="dot dot-close"></div>
          <div className="dot dot-minimize"></div>
          <div className="dot dot-maximize"></div>
        </div>

        {/* Brand Header */}
        {!isWindows && (
          <div className="sidebar-brand">
            <h2 className="brand-title">TermDock</h2>
            <span className="brand-version">v{desktopApi?.appVersion ?? '—'}</span>
          </div>
        )}

        {/* Navigation Section */}
        <nav className="sidebar-nav">
          <button
            className={`sidebar-nav-link ${activeTab === 'overview' ? 'active' : ''}`}
            onClick={() => selectTab('overview')}
            type="button"
          >
            <span className="material-symbols-outlined">dashboard</span>
            <span>概览</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'quick-links' ? 'active' : ''}`}
            onClick={() => selectTab('quick-links')}
            type="button"
          >
            <span className="material-symbols-outlined">link</span>
            <span>{t.quickConnect}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'command-manager' ? 'active' : ''}`}
            onClick={() => selectTab('command-manager')}
            type="button"
          >
            <span className="material-symbols-outlined">terminal</span>
            <span>{t.commandManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'connection-manager' ? 'active' : ''}`}
            onClick={() => selectTab('connection-manager')}
            type="button"
          >
            <span className="material-symbols-outlined">settings_ethernet</span>
            <span>{t.connectionManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => selectTab('settings')}
            type="button"
          >
            <span className="material-symbols-outlined">settings</span>
            <span>{t.settings}</span>
          </button>
        </nav>

        {/* Sidebar Footer */}
        <div className="sidebar-footer">
          <button className="sidebar-nav-link" onClick={handleOpenDocs} type="button">
            <span className="material-symbols-outlined">description</span>
            <span>Docs</span>
          </button>
          <button className="sidebar-nav-link" onClick={handleOpenDocs} type="button">
            <span className="material-symbols-outlined">star</span>
            <span>GitHub</span>
          </button>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="home-main-content">
        <div className="home-tabs-bar">
          <TabBar {...tabBarProps} />
        </div>
        <div className="home-content-body scrollbar-scroll">
          {activeTab === 'overview' && (
            <div key="overview" className="page-transition" data-nav-direction={navDirection}>
              <OverviewPage
                profiles={profiles}
                folders={folders}
                onOpenProfile={onOpen}
                onOpenNewConnection={handleOpenNewConnection}
                onOpenConnectionManager={() => selectTab('connection-manager')}
                onOpenCommandManager={() => selectTab('command-manager')}
                onOpenDocs={handleOpenDocs}
              />
            </div>
          )}
          {activeTab === 'quick-links' && (
            <div key="quick-links" className="page-transition" data-nav-direction={navDirection}>
              <QuickLinksPage
                profiles={profiles}
                folders={folders}
                onOpen={onOpen}
                onOpenNewConnection={handleOpenNewConnection}
              />
            </div>
          )}
          {activeTab === 'connection-manager' && (
            <div key="connection-manager" className="page-transition" data-nav-direction={navDirection}>
              <ConnectionManagerModal
                profiles={profiles}
                folders={folders}
                onClose={() => selectTab('overview')}
                onCreate={onCreateConnection}
                onDeleteProfile={onDeleteConnection}
                onEditProfile={onEditConnection}
                onOpenProfile={onOpen}
                onCreateFolder={onCreateConnectionFolder}
                onDeleteFolder={onDeleteConnectionFolder}
                onUpdateFolder={onUpdateConnectionFolder}
                onUpdateOrder={onUpdateConnectionOrder}
                inline={true}
                onActiveFolderChange={setActiveConnectionFolderName}
              />
            </div>
          )}
          {activeTab === 'command-manager' && (
            <div key="command-manager" className="page-transition" data-nav-direction={navDirection}>
              <CommandManagerModal
                commandFolders={commandFolders}
                commandTemplates={commandTemplates}
                onClose={() => selectTab('overview')}
                onCreateCommand={onCreateCommand}
                onUpdateCommand={onUpdateCommand}
                onDeleteCommand={onDeleteCommand}
                onCreateFolder={onCreateCommandFolder}
                onDeleteFolder={onDeleteCommandFolder}
                onUpdateFolder={onUpdateCommandFolder}
                onUpdateOrder={onUpdateCommandOrder}
                inline={true}
                onActiveFolderChange={setActiveCommandFolderName}
              />
            </div>
          )}
          {activeTab === 'settings' && (
            <div key="settings" className="page-transition" data-nav-direction={navDirection}>
              <SettingsModal
                theme={theme}
                onSetTheme={onSetTheme}
                locale={locale}
                onSetLocale={onSetLocale}
                onOpenCommandManager={() => selectTab('command-manager')}
                onOpenConnectionManager={() => selectTab('connection-manager')}
                onOpenLogsDirectory={onOpenLogsDirectory}
                onClose={() => selectTab('overview')}
                inline={true}
              />
            </div>
          )}
        </div>

        {/* Custom Footer */}
        <footer className="home-footer">
          <div className="footer-copyright">
            <span>© 2026 TermDock Team. MIT Licensed. System: 0.1ms latency</span>
            {activeTab === 'connection-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>{profiles.length} {t.connectionCountLabel}</span>
                <span className="footer-meta-separator">|</span>
                <span>{folders.length} {t.folderCountLabel}</span>
                {activeConnectionFolderName && (
                  <>
                    <span className="footer-meta-separator">|</span>
                    <span>{activeConnectionFolderName}</span>
                  </>
                )}
              </>
            )}
            {activeTab === 'command-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>{commandTemplates.length} {t.commandCountLabel}</span>
                <span className="footer-meta-separator">|</span>
                <span>{commandFolders.length} {t.folderCountLabel}</span>
                {activeCommandFolderName && (
                  <>
                    <span className="footer-meta-separator">|</span>
                    <span>{activeCommandFolderName}</span>
                  </>
                )}
              </>
            )}
          </div>
          <nav className="footer-nav">
            <button onClick={handleOpenDocs} type="button">Changelog</button>
            <button onClick={handleOpenDocs} type="button">API Reference</button>
            <button className="footer-status-link" type="button">
              <span className="footer-status-dot"></span>
              <span>Status</span>
            </button>
          </nav>
        </footer>
      </main>
    </section>
  )
}
