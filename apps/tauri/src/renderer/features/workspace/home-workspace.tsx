import type {
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  McpAgentClientStatus,
  OverviewSectionId,
  AppUpdateStatus,
  SavedTheme,
  SessionSnapshot,
  ThemeConfig,
  WorkspaceTab
} from '@fileterm/core'
import { useEffect, useRef, useState } from 'react'
import type { ThemeMode } from '../../app/theme-config'
import { t } from '../../i18n'
import { resolveRendererPlatform } from '../../lib/renderer-platform'
import { AppIcon } from '../common/app-icon'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { PortableUpdateDialog } from '../common/portable-update-dialog'
import { OverviewPage } from './overview-page'
import { QuickLinksPage } from './quick-links-page'
import { ConnectionManagerModal } from '../connections/connection-manager-modal'
import { CommandManagerModal } from '../commands/command-manager-modal'
import { SshKeyManagerPage } from '../ssh-keys/ssh-key-manager-page'
import { SettingsModal } from '../settings/settings-modal'
import { TabBar, type TabBarProps } from '../layout/tab-bar'
import { BackgroundSessionsPage } from './background-sessions-page'

export function HomeWorkspace({
  profiles,
  backgroundTabs,
  sessions,
  folders = [],
  commandFolders = [],
  commandTemplates = [],
  theme,
  themeConfig,
  customThemes,
  locale,
  overviewShowStats,
  overviewShowRecent,
  overviewShowAllConnections,
  overviewShowQuickActions,
  overviewSectionOrder,
  onOpen,
  onOpenLocalTerminal,
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
  onSetThemeConfig,
  onSetCustomThemes,
  onSetLocale,
  onOpenLogsDirectory,
  onLaunchLocalAgent,
  canLockNow,
  onLockNow,
  isSidebarCollapsed,
  tabBarProps,
  isResizingSidebar,
  onResizeStart,
  onAttachBackgroundSession,
  onCloseBackgroundSession
}: {
  profiles: ConnectionProfile[]
  backgroundTabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  folders?: ConnectionFolder[]
  commandFolders?: CommandFolder[]
  commandTemplates?: CommandTemplate[]
  theme: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  locale: 'zhCN' | 'enUS'
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  onOpen(profileId: string): void
  onOpenLocalTerminal(): void
  onCreateConnection(): void
  onEditConnection(profile: ConnectionProfile): void
  onDeleteConnection(profileId: string): Promise<boolean> | boolean | void
  onCreateConnectionFolder(name: string): Promise<boolean> | boolean | void
  onDeleteConnectionFolder(folderId: string): Promise<boolean> | boolean | void
  onUpdateConnectionFolder(folderId: string, updates: Partial<ConnectionFolder>): Promise<boolean> | boolean | void
  onUpdateConnectionOrder(
    id: string,
    newParentId: string | undefined,
    newOrder: number
  ): Promise<boolean> | boolean | void
  onImportConnections(source?: 'files' | 'folder'): void
  onExportConnections(): void
  onCreateCommand(input: CommandTemplateInput): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
  onDeleteCommand(commandId: string): Promise<boolean> | boolean | void
  onCreateCommandFolder(name: string): Promise<boolean> | boolean | void
  onDeleteCommandFolder(folderId: string): Promise<boolean> | boolean | void
  onUpdateCommandFolder(folderId: string, updates: Partial<CommandFolder>): Promise<boolean> | boolean | void
  onUpdateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<boolean> | boolean | void
  onSetTheme(value: ThemeMode): void
  onSetThemeConfig(value: ThemeConfig): void
  onSetCustomThemes(value: SavedTheme[]): void
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  onLaunchLocalAgent?(client: McpAgentClientStatus): void
  canLockNow: boolean
  onLockNow(): void
  isSidebarCollapsed: boolean
  tabBarProps: Omit<TabBarProps, 'homeBrandContent'>
  isResizingSidebar: boolean
  onResizeStart(): void
  onAttachBackgroundSession(tabId: string): void | Promise<void>
  onCloseBackgroundSession(tabId: string): void | Promise<void>
}) {
  const [activeTab, setActiveTab] = useState<
    | 'overview'
    | 'quick-links'
    | 'command-manager'
    | 'connection-manager'
    | 'ssh-key-manager'
    | 'background-sessions'
    | 'settings'
  >('overview')
  const contentBodyRef = useRef<HTMLDivElement | null>(null)
  const [navDirection, setNavDirection] = useState<'down' | 'up'>('down')
  const [activeConnectionFolderName, setActiveConnectionFolderName] = useState('')
  const [activeCommandFolderName, setActiveCommandFolderName] = useState('')
  const [activeSshKeyFolderName, setActiveSshKeyFolderName] = useState(t.allKeys)
  const [sshKeyStats, setSshKeyStats] = useState({ keyCount: 0, folderCount: 0 })

  // 侧栏页签的纵向顺序,用于判断切换方向(目标更靠下=向下飞入,更靠上=向上飞入)
  const tabOrder: Record<string, number> = {
    overview: 0,
    'connection-manager': 1,
    'command-manager': 2,
    'ssh-key-manager': 3,
    'background-sessions': 4,
    settings: 5,
    'quick-links': 6
  }
  const selectTab = (tab: typeof activeTab) => {
    if (tab === activeTab) return
    setNavDirection((tabOrder[tab] ?? 0) >= (tabOrder[activeTab] ?? 0) ? 'down' : 'up')
    setActiveTab(tab)
  }

  const desktopApi = window.fileterm
  // Keep Linux on the same compact home-shell layout as Windows. macOS keeps
  // its sidebar wordmark because it has native traffic lights above it.
  const rendererPlatform = resolveRendererPlatform(desktopApi?.platform ?? 'browser')
  const usesWindowsHomeLayout = rendererPlatform === 'win32' || rendererPlatform === 'linux'
  const updatePreviewState = import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [isPortableUpdateDialogOpen, setPortableUpdateDialogOpen] = useState(false)

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
        message: updatePreviewState === 'error' ? t.updateServerUnavailable : undefined
      }
      setUpdateStatus(previewStatus)
      return
    }
    if (!desktopApi) return
    void desktopApi.getUpdateStatus().then(setUpdateStatus)
    return desktopApi.onUpdateStatus(setUpdateStatus)
  }, [desktopApi, updatePreviewState])

  const homeBrandContent = usesWindowsHomeLayout ? (
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
        if (updateStatus.isPortable) {
          setPortableUpdateDialogOpen(true)
        } else {
          void desktopApi?.openExternalUrl(updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
        }
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
        <TabBar {...tabBarProps} homeBrandContent={homeBrandContent} showAiCopilotToggle={false} />
      </div>

      {/* SideNavBar Component */}
      <aside className={`home-sidebar ${isSidebarCollapsed ? 'is-collapsed' : ''}`}>
        {!usesWindowsHomeLayout ? (
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
            aria-label={t.overview}
            title={t.overview}
            type="button"
          >
            <span className="material-symbols-outlined">dashboard</span>
            <span>{t.overview}</span>
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
            aria-label={t.sshKeyManager}
            title={t.sshKeyManager}
            type="button"
          >
            <span className="material-symbols-outlined">key</span>
            <span>{t.sshKeyManager}</span>
          </button>
          <button
            className={`sidebar-nav-link ${activeTab === 'background-sessions' ? 'active' : ''}`}
            onClick={() => selectTab('background-sessions')}
            aria-label={t.backgroundSessions}
            title={t.backgroundSessions}
            type="button"
          >
            <AppIcon name="history" size={18} />
            <span>{t.backgroundSessions}</span>
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
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
            onResizeStart()
          }}
          role="separator"
        />
      </aside>

      {/* Main Content Area */}
      <main className="home-main-content">
        <div className="home-content-body scrollbar-scroll" ref={contentBodyRef}>
          {activeTab === 'overview' && (
            <div key="overview" className="page-transition" data-nav-direction={navDirection}>
              <OverviewPage
                profiles={profiles}
                folders={folders}
                onOpenProfile={onOpen}
                showStats={overviewShowStats}
                showRecent={overviewShowRecent}
                showAllConnections={overviewShowAllConnections}
                showQuickActions={overviewShowQuickActions}
                sectionOrder={overviewSectionOrder}
                onOpenLocalTerminal={onOpenLocalTerminal}
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
          {activeTab === 'background-sessions' && (
            <div key="background-sessions" className="page-transition" data-nav-direction={navDirection}>
              <BackgroundSessionsPage
                profiles={profiles}
                tabs={backgroundTabs}
                sessions={sessions}
                onAttach={onAttachBackgroundSession}
                onClose={onCloseBackgroundSession}
              />
            </div>
          )}
          {activeTab === 'settings' && (
            <div key="settings" className="page-transition" data-nav-direction={navDirection}>
              <SettingsModal
                theme={theme}
                themeConfig={themeConfig}
                customThemes={customThemes}
                onSetTheme={onSetTheme}
                onSetThemeConfig={onSetThemeConfig}
                onSetCustomThemes={onSetCustomThemes}
                locale={locale}
                onSetLocale={onSetLocale}
                onOpenCommandManager={() => selectTab('command-manager')}
                onOpenConnectionManager={() => selectTab('connection-manager')}
                onOpenLogsDirectory={onOpenLogsDirectory}
                onLaunchLocalAgent={onLaunchLocalAgent}
                onClose={() => selectTab('overview')}
                inline={true}
              />
            </div>
          )}
        </div>
        <VerticalScrollbar ariaLabel={t.overview} scrollRef={contentBodyRef} />

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
                <span>{t.keyCount.replace('{count}', String(sshKeyStats.keyCount))}</span>
                <span className="footer-meta-separator">|</span>
                <span>{t.folderCount.replace('{count}', String(sshKeyStats.folderCount))}</span>
                <span className="footer-meta-spacer"></span>
                <span>{activeSshKeyFolderName}</span>
              </>
            )}
            {canLockNow ? (
              <button
                aria-label={t.securityLockNow}
                className="security-lock-now-button"
                onClick={onLockNow}
                title={t.securityLockNow}
                type="button"
              >
                <AppIcon name="lock" size={15} />
              </button>
            ) : null}
          </div>
          {/* Footer nav hidden: Changelog, API Reference, Status — code/handlers preserved */}
        </footer>
      </main>
      {isPortableUpdateDialogOpen ? (
        <PortableUpdateDialog
          onClose={() => setPortableUpdateDialogOpen(false)}
          onOpenReleasePage={() => {
            if (import.meta.env.DEV && !updateStatus?.isPortable) return
            void desktopApi?.openExternalUrl(updateStatus?.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
          }}
        />
      ) : null}
    </section>
  )
}
