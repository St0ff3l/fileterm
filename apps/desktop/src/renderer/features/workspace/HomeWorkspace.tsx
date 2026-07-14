import type {
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  AppUpdateStatus
} from '@fileterm/core'
import { useEffect, useState } from 'react'
import { t } from '../../i18n'
import { OverviewPage } from './OverviewPage'
import { QuickLinksPage } from './QuickLinksPage'
import { ConnectionManagerModal } from '../connections/ConnectionManagerModal'
import { CommandManagerModal } from '../commands/CommandManagerModal'
import { SshKeyManagerPage } from '../ssh-keys/SshKeyManagerPage'
import { SettingsModal } from '../settings/SettingsModal'
import { TabBar, type TabBarProps } from '../layout/TabBar'

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
  onImportConnections,
  onExportConnections,
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
  isSidebarCollapsed,
  tabBarProps,
  isResizingSidebar,
  onResizeStart
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
  onImportConnections(): void
  onExportConnections(): void
  onCreateCommand(input: CommandTemplateInput): void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): void
  onDeleteCommand(commandId: string): void
  onCreateCommandFolder(name: string): void
  onDeleteCommandFolder(folderId: string): void
  onUpdateCommandFolder(folderId: string, updates: Partial<CommandFolder>): void
  onUpdateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onSetTheme(value: 'default-dark' | 'default-light'): void
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  isSidebarCollapsed: boolean
  tabBarProps: Omit<TabBarProps, 'homeBrandContent'>
  isResizingSidebar: boolean
  onResizeStart(): void
}) {
  const [activeTab, setActiveTab] = useState<
    'overview' | 'quick-links' | 'command-manager' | 'connection-manager' | 'ssh-key-manager' | 'settings'
  >('overview')
  const [navDirection, setNavDirection] = useState<'down' | 'up'>('down')
  const [activeConnectionFolderName, setActiveConnectionFolderName] = useState('')
  const [activeCommandFolderName, setActiveCommandFolderName] = useState('')
  const [activeSshKeyFolderName, setActiveSshKeyFolderName] = useState('全部密钥')
  const [sshKeyStats, setSshKeyStats] = useState({ keyCount: 0, folderCount: 0 })

  // 侧栏页签的纵向顺序,用于判断切换方向(目标更靠下=向下飞入,更靠上=向上飞入)
  const tabOrder: Record<string, number> = {
    overview: 0,
    'connection-manager': 1,
    'command-manager': 2,
    'ssh-key-manager': 3,
    settings: 4,
    'quick-links': 5
  }
  const selectTab = (tab: typeof activeTab) => {
    if (tab === activeTab) return
    setNavDirection((tabOrder[tab] ?? 0) >= (tabOrder[activeTab] ?? 0) ? 'down' : 'up')
    setActiveTab(tab)
  }

  const desktopApi = window.fileterm
  const isWindows = desktopApi?.platform === 'win32'
  const updatePreviewState = import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)

  useEffect(() => {
    if (updatePreviewState) {
      const previewStatus: AppUpdateStatus = {
        currentVersion: desktopApi?.appVersion ?? '1.0.0',
        state:
          updatePreviewState === 'downloading' || updatePreviewState === 'downloaded' || updatePreviewState === 'error'
            ? updatePreviewState
            : 'available',
        availableVersion: '1.1.0',
        progress: updatePreviewState === 'downloading' ? 62 : updatePreviewState === 'downloaded' ? 100 : undefined,
        message: updatePreviewState === 'error' ? '无法连接到更新服务器' : undefined
      }
      setUpdateStatus(previewStatus)
      return
    }
    if (!desktopApi) return
    void desktopApi.getUpdateStatus().then(setUpdateStatus)
    return desktopApi.onUpdateStatus(setUpdateStatus)
  }, [desktopApi, updatePreviewState])

  const homeBrandContent = isWindows ? (
    <>
      <strong>{desktopApi?.appName ?? 'FileTerm'}</strong>
      <span>v{desktopApi?.appVersion ?? '0.0.0'}</span>
    </>
  ) : undefined

  const handleOpenNewConnection = () => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('create')
    }
  }

  const handleOpenDocs = () => {
    if (desktopApi) {
      void desktopApi.openExternalUrl('https://github.com/St0ff3l/fileterm')
    }
  }

  const updateAction = () => {
    if (updateStatus?.state === 'available') {
      if (updateStatus.updateMode === 'release-page') {
        void desktopApi?.openExternalUrl(updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
      } else {
        void desktopApi?.downloadUpdate()
      }
    } else if (updateStatus?.state === 'downloaded') {
      void desktopApi?.installUpdate()
    } else if (updateStatus?.state === 'error') {
      void desktopApi?.checkForUpdates()
    }
  }

  const updateHint =
    updateStatus?.state === 'available'
      ? {
          icon: 'system_update',
          label: t.updateAvailableShort.replace('{version}', updateStatus.availableVersion ?? '—'),
          title: updateStatus.updateMode === 'release-page' ? t.openReleasePage : t.downloadUpdate
        }
      : updateStatus?.state === 'downloading'
        ? {
            icon: 'downloading',
            label: t.updateDownloadingShort.replace('{progress}', String(updateStatus.progress ?? 0)),
            title: t.updateDownloading.replace('{progress}', String(updateStatus.progress ?? 0))
          }
        : updateStatus?.state === 'downloaded'
          ? { icon: 'restart_alt', label: t.restartToUpdate, title: t.restartToUpdate }
          : updateStatus?.state === 'error'
            ? { icon: 'error_outline', label: t.updateRetry, title: updateStatus.message ?? t.updateFailed }
            : null

  return (
    <section className={`home-workspace ${isSidebarCollapsed ? 'is-sidebar-collapsed' : ''}`}>
      <div className="home-tabs-bar">
        <TabBar {...tabBarProps} homeBrandContent={homeBrandContent} />
      </div>

      {/* SideNavBar Component */}
      <aside className={`home-sidebar ${isSidebarCollapsed ? 'is-collapsed' : ''}`}>
        {!isWindows ? (
          <div className="sidebar-brand">
            <h2 className="brand-title">FileTerm</h2>
            <span className="brand-version">v{desktopApi?.appVersion ?? '—'}</span>
          </div>
        ) : null}

        {/* Navigation Section */}
        <nav className="sidebar-nav">
          <button
            className={`sidebar-nav-link ${activeTab === 'overview' ? 'active' : ''}`}
            onClick={() => selectTab('overview')}
            aria-label="概览"
            title="概览"
            type="button"
          >
            <span className="material-symbols-outlined">dashboard</span>
            <span>概览</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'connection-manager' ? 'active' : ''}`}
            onClick={() => selectTab('connection-manager')}
            aria-label={t.connectionManager}
            title={t.connectionManager}
            type="button"
          >
            <span className="material-symbols-outlined">settings_ethernet</span>
            <span>{t.connectionManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'command-manager' ? 'active' : ''}`}
            onClick={() => selectTab('command-manager')}
            aria-label={t.commandManager}
            title={t.commandManager}
            type="button"
          >
            <span className="material-symbols-outlined">terminal</span>
            <span>{t.commandManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'ssh-key-manager' ? 'active' : ''}`}
            onClick={() => selectTab('ssh-key-manager')}
            aria-label="密钥管理器"
            title="密钥管理器"
            type="button"
          >
            <span className="material-symbols-outlined">key</span>
            <span>密钥管理器</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => selectTab('settings')}
            aria-label={t.settings}
            title={t.settings}
            type="button"
          >
            <span className="material-symbols-outlined">settings</span>
            <span>{t.settings}</span>
          </button>
          {/* Quick Connect sidebar button hidden — code preserved in QuickLinksPage.tsx */}
          {/* Docs sidebar button hidden — handleOpenDocs still available */}
        </nav>

        {/* Sidebar Footer */}
        <div className="sidebar-footer">
          <button
            className="sidebar-nav-link"
            onClick={handleOpenDocs}
            aria-label="GitHub"
            title="GitHub"
            type="button"
          >
            <span className="material-symbols-outlined">star</span>
            <span>GitHub</span>
          </button>
          {updateHint ? (
            <button
              aria-label={updateHint.title}
              className={`sidebar-update-hint is-${updateStatus?.state}`}
              disabled={updateStatus?.state === 'downloading'}
              onClick={updateAction}
              title={updateHint.title}
              type="button"
            >
              <span aria-hidden="true" className="material-symbols-outlined">
                {updateHint.icon}
              </span>
              <span>{updateHint.label}</span>
            </button>
          ) : null}
        </div>
        <div
          aria-label={t.resizeSidebar}
          className={`sidebar-resizer ${isResizingSidebar ? 'is-active' : ''}`}
          onMouseDown={onResizeStart}
          role="separator"
        />
      </aside>

      {/* Main Content Area */}
      <main className="home-main-content">
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
                onImportConnections={onImportConnections}
                onExportConnections={onExportConnections}
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
          {activeTab === 'ssh-key-manager' && (
            <div key="ssh-key-manager" className="page-transition" data-nav-direction={navDirection}>
              <SshKeyManagerPage onActiveFolderChange={setActiveSshKeyFolderName} onStatsChange={setSshKeyStats} />
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
            <span>© 2026 FileTerm Team. MIT Licensed.</span>
            {activeTab === 'connection-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>
                  {profiles.length} {t.connectionCountLabel}
                </span>
                <span className="footer-meta-separator">|</span>
                <span>
                  {folders.length} {t.folderCountLabel}
                </span>
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
                <span>
                  {commandTemplates.length} {t.commandCountLabel}
                </span>
                <span className="footer-meta-separator">|</span>
                <span>
                  {commandFolders.length} {t.folderCountLabel}
                </span>
                {activeCommandFolderName && (
                  <>
                    <span className="footer-meta-separator">|</span>
                    <span>{activeCommandFolderName}</span>
                  </>
                )}
              </>
            )}
            {activeTab === 'ssh-key-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>{sshKeyStats.keyCount} 个密钥</span>
                <span className="footer-meta-separator">|</span>
                <span>{sshKeyStats.folderCount} 个分组</span>
                <span className="footer-meta-spacer"></span>
                <span>{activeSshKeyFolderName}</span>
              </>
            )}
          </div>
          {/* Footer nav hidden: Changelog, API Reference, Status — code/handlers preserved */}
        </footer>
      </main>
    </section>
  )
}
