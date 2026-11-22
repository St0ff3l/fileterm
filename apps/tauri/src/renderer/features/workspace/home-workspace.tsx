import { MaterialIcon, type MaterialIconName } from '../../components/common/material-icon/material-icon'
import { SidebarAlignedLabel } from '../../components/common/icon-label/icon-label'
import './home-workspace.css'
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
  UiPreferences,
  WorkspaceTab
} from '@fileterm/core'
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { ThemeMode } from '../../app/theme-config'
import { t } from '../../i18n'
import { resolveRendererPlatform } from '../../lib/renderer-platform'
import { useExternalUrl } from '../../hooks/use-external-url'
import { FeedbackText } from '../common/feedback-text'
import { AppIcon } from '../common/app-icon'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { PortableUpdateDialog } from '../common/portable-update-dialog'
import { OverviewPage } from './overview-page'
import { ConnectionManagerModal } from '../connections/connection-manager-modal'
import { ProxyManagerPage, type ProxyManagerStats } from '../proxies/proxy-manager-page'
import { CommandManagerModal } from '../commands/command-manager-modal'
import { SshKeyManagerPage } from '../ssh-keys/ssh-key-manager-page'
import { McpCallLogPage } from './mcp-call-log-page'
import { SettingsModal } from '../settings/settings-modal'
import { getHomeBrandContent, TabBar, type TabBarProps } from '../layout/tab-bar'
import { BackgroundSessionsPage } from './background-sessions-page'
import type { ServerManagementNavigation } from '../server-management/server-management-types'
import { ServerTopologyPage } from '../server-management/server-topology-page'
import { ServerManagementOverview } from '../server-management/server-management-overview'

export function HomeWorkspace({
  profiles,
  backgroundSessionsRequest,
  backgroundTabs,
  sessions,
  folders = [],
  commandFolders = [],
  commandTemplates = [],
  theme,
  themeConfig,
  customThemes,
  uiPreferences,
  locale,
  maskConnectionAddresses,
  suppressEntrance: suppressEntranceProp,
  overviewShowStats,
  overviewShowRecent,
  overviewShowAllConnections,
  overviewShowQuickActions,
  overviewSectionOrder,
  onOpen,
  onConnectProfile,
  onDisconnectSession,
  onOpenFileTransfers,
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
  isSidebarCollapsed,
  isWorkspaceActive,
  serverManagement,
  tabBarProps,
  isResizingSidebar,
  onResizeStart,
  onAttachBackgroundSession,
  onCloseBackgroundSession
}: {
  profiles: ConnectionProfile[]
  backgroundSessionsRequest?: number
  backgroundTabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  folders?: ConnectionFolder[]
  commandFolders?: CommandFolder[]
  commandTemplates?: CommandTemplate[]
  theme: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  uiPreferences: UiPreferences | undefined
  locale: 'zhCN' | 'enUS'
  maskConnectionAddresses: boolean
  suppressEntrance?: boolean
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  onOpen(profileId: string): void
  onConnectProfile(profileId: string): void
  onDisconnectSession(tabId: string): void | Promise<void>
  onOpenFileTransfers(): void
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
  isSidebarCollapsed: boolean
  isWorkspaceActive: boolean
  serverManagement?: ServerManagementNavigation
  tabBarProps: Omit<TabBarProps, 'homeBrandContent'>
  isResizingSidebar: boolean
  onResizeStart(): void
  onAttachBackgroundSession(tabId: string): void | Promise<void>
  onCloseBackgroundSession(tabId: string): void | Promise<void>
}) {
  const isServerManagementMode = Boolean(serverManagement)
  const managementPage = serverManagement?.page ?? 'overview'
  const [activeTab, setActiveTab] = useState<
    | 'overview'
    | 'connection-manager'
    | 'proxy-manager'
    | 'command-manager'
    | 'ssh-key-manager'
    | 'background-sessions'
    | 'mcp-call-log'
    | 'settings'
  >(backgroundSessionsRequest === undefined ? 'overview' : 'background-sessions')
  useLayoutEffect(() => {
    if (backgroundSessionsRequest !== undefined) {
      setActiveTab('background-sessions')
      if (isServerManagementMode) serverManagement?.navigate('background-sessions')
    }
  }, [backgroundSessionsRequest, isServerManagementMode, serverManagement?.navigate])
  const contentBodyRef = useRef<HTMLDivElement | null>(null)
  const [navDirection, setNavDirection] = useState<'down' | 'up'>('down')
  const [suppressEntrance, setSuppressEntrance] = useState(Boolean(suppressEntranceProp))
  const [activeConnectionFolderName, setActiveConnectionFolderName] = useState('')
  const [activeCommandFolderName, setActiveCommandFolderName] = useState('')
  const [activeSshKeyFolderName, setActiveSshKeyFolderName] = useState(t.allKeys)
  const [sshKeyStats, setSshKeyStats] = useState({ keyCount: 0, folderCount: 0 })
  const [mcpCallCount, setMcpCallCount] = useState(0)
  const [activeProxyFilterLabel, setActiveProxyFilterLabel] = useState(t.allProxiesAndTunnels)
  const [proxyStats, setProxyStats] = useState<ProxyManagerStats>({
    totalCount: 0,
    proxyCount: 0,
    tunnelCount: 0,
    socks5Count: 0,
    httpProxyCount: 0,
    sshTunnelCount: 0,
    httpTunnelCount: 0
  })

  // 侧栏页签的纵向顺序,用于判断切换方向(目标更靠下=向下飞入,更靠上=向上飞入)
  const tabOrder: Record<string, number> = {
    overview: 0,
    'server-topology': 1,
    'connection-manager': 2,
    'proxy-manager': 3,
    'command-manager': 4,
    'ssh-key-manager': 5,
    'background-sessions': 6,
    'mcp-call-log': 7,
    settings: 8
  }
  const resolveNavDirection = (fromPage: string, toPage: string) =>
    (tabOrder[toPage] ?? 0) >= (tabOrder[fromPage] ?? 0) ? 'down' : 'up'
  const selectTab = (tab: typeof activeTab) => {
    if (tab === currentPage) return
    setSuppressEntrance(false)
    setNavDirection(resolveNavDirection(currentPage, tab))
    setActiveTab(tab)
    if (isServerManagementMode) serverManagement?.navigate(tab)
  }

  // Management navigation lives in the host rail, so it bypasses selectTab().
  // Resolve the direction during render and remount the transition wrapper on
  // every page change; otherwise revisits reuse the finished animation state
  // and the fly-in disappears. React applies these render-phase updates before
  // the new page commits, so the wrapper mounts with the right direction and
  // without the suppressed-entrance class.
  const requestedPage = isServerManagementMode ? managementPage : activeTab
  const pageEntryRef = useRef({ page: requestedPage, seq: 0 })
  const previousManagementMode = useRef(isServerManagementMode)
  const keepSettings =
    isWorkspaceActive &&
    previousManagementMode.current !== isServerManagementMode &&
    pageEntryRef.current.page === 'settings'
  const currentPage = keepSettings ? 'settings' : requestedPage
  const navigateManagement = serverManagement?.navigate
  useLayoutEffect(() => {
    const didChangeMode = previousManagementMode.current !== isServerManagementMode
    previousManagementMode.current = isServerManagementMode
    if (didChangeMode) setSuppressEntrance(false)
    if (!keepSettings) return
    if (navigateManagement) navigateManagement('settings')
    else setActiveTab('settings')
  }, [isServerManagementMode, keepSettings, navigateManagement])
  if (pageEntryRef.current.page !== currentPage) {
    const previousPage = pageEntryRef.current.page
    pageEntryRef.current = { page: currentPage, seq: pageEntryRef.current.seq + 1 }
    setNavDirection(resolveNavDirection(previousPage, currentPage))
    if (suppressEntrance) setSuppressEntrance(false)
  }
  const pageEntryKey = (page: string) => `${page}:${pageEntryRef.current.seq}`
  const pageEntrySeq = pageEntryRef.current.seq
  // React can reuse page nodes across page and workspace-mode changes, so replay
  // the fly-in explicitly after commit instead of leaving a finished animation in place.
  useLayoutEffect(() => {
    const pageRoot = contentBodyRef.current?.querySelector(':scope > .page-transition > *')
    if (!pageRoot) return
    for (const child of Array.from(pageRoot.children) as HTMLElement[]) {
      child.style.animation = 'none'
      void child.offsetWidth
      child.style.animation = ''
    }
  }, [isServerManagementMode, pageEntrySeq])

  const desktopApi = window.fileterm
  // Sessions the dashboard owns: visible tabs plus the background connections
  // created by the connect action.
  const managementTabs = [
    ...tabBarProps.orderedTabs.flatMap((entry) => (entry.kind === 'session' ? [entry.tab] : [])),
    ...backgroundTabs
  ]
  const lastActiveSessionTabId = useRef<string | null>(null)
  if (tabBarProps.activeSessionTabId) {
    lastActiveSessionTabId.current = tabBarProps.activeSessionTabId
  }
  // Remember the server and the exact session tab the user came from so
  // returning to the overview lands on the matching connection, and the
  // terminal entry reopens the tab that was last active.
  const lastActiveSessionTab = managementTabs.find(
    (tab) => tab.id === (tabBarProps.activeSessionTabId ?? lastActiveSessionTabId.current)
  )
  const focusedProfileId = lastActiveSessionTab?.profileId ?? null
  // Bumping this request on every return from a terminal re-applies the
  // remembered server even if the selection was changed manually in between.
  const [overviewFocusRequest, setOverviewFocusRequest] = useState(0)
  const previousActiveSessionTabId = useRef<string | null>(tabBarProps.activeSessionTabId)
  useEffect(() => {
    const previous = previousActiveSessionTabId.current
    previousActiveSessionTabId.current = tabBarProps.activeSessionTabId
    if (isServerManagementMode && previous && !tabBarProps.activeSessionTabId) {
      setOverviewFocusRequest((current) => current + 1)
    }
  }, [isServerManagementMode, tabBarProps.activeSessionTabId])
  const { openExternalUrl, externalUrlError } = useExternalUrl(desktopApi, t.openExternalUrlFailed)
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

  const homeBrandContent = usesWindowsHomeLayout
    ? getHomeBrandContent(desktopApi?.appName, desktopApi?.appVersion)
    : undefined

  const handleOpenNewConnection = () => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('create')
    }
  }

  const handleOpenDocs = () => {
    if (desktopApi) {
      void openExternalUrl('https://github.com/St0ff3l/fileterm')
    }
  }

  const updateAction = () => {
    if (updateStatus?.state === 'available') {
      if (updateStatus.updateMode === 'release-page') {
        if (updateStatus.isPortable) {
          setPortableUpdateDialogOpen(true)
        } else {
          void openExternalUrl(updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
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

  const updateHint: { icon: MaterialIconName; label: string; title: string } | null =
    updateStatus?.state === 'available'
      ? {
          icon: 'system_update_alt',
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
            ? { icon: 'error', label: t.updateRetry, title: updateStatus.message ?? t.updateFailed }
            : null

  return (
    <section
      className={`home-workspace ${isSidebarCollapsed ? 'is-sidebar-collapsed' : ''} ${isServerManagementMode ? 'is-management-mode' : ''}`}
    >
      <div className="home-tabs-bar" hidden={isServerManagementMode}>
        <TabBar {...tabBarProps} homeBrandContent={homeBrandContent} showAiCopilotToggle={false} />
      </div>

      {/* SideNavBar Component */}
      <aside className={`home-sidebar ${isSidebarCollapsed ? 'is-collapsed' : ''}`} hidden={isServerManagementMode}>
        {!usesWindowsHomeLayout ? (
          <div className="sidebar-brand">
            <h2 className="brand-title">FileTerm</h2>
            <span className="brand-version">v{desktopApi?.appVersion ?? '—'}</span>
          </div>
        ) : null}

        {/* Navigation Section */}
        <nav className="sidebar-nav">
          <button
            className={`sidebar-nav-link ${currentPage === 'overview' ? 'active' : ''}`}
            onClick={() => selectTab('overview')}
            aria-label={t.overview}
            title={t.overview}
            type="button"
          >
            <MaterialIcon name="dashboard" className="material-symbols-outlined" />
            <SidebarAlignedLabel>{t.overview}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'connection-manager' ? 'active' : ''}`}
            onClick={() => selectTab('connection-manager')}
            aria-label={t.connectionManager}
            title={t.connectionManager}
            type="button"
          >
            <MaterialIcon name="settings_ethernet" className="material-symbols-outlined" />
            <SidebarAlignedLabel>{t.connectionManager}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'proxy-manager' ? 'active' : ''}`}
            onClick={() => selectTab('proxy-manager')}
            aria-label={t.proxyManager}
            title={t.proxyManager}
            type="button"
          >
            <AppIcon name="shield" size={20} />
            <SidebarAlignedLabel>{t.proxyManager}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'command-manager' ? 'active' : ''}`}
            onClick={() => selectTab('command-manager')}
            aria-label={t.commandManager}
            title={t.commandManager}
            type="button"
          >
            <MaterialIcon name="terminal" className="material-symbols-outlined" />
            <SidebarAlignedLabel>{t.commandManager}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'ssh-key-manager' ? 'active' : ''}`}
            onClick={() => selectTab('ssh-key-manager')}
            aria-label={t.sshKeyManager}
            title={t.sshKeyManager}
            type="button"
          >
            <MaterialIcon name="key" className="material-symbols-outlined" />
            <SidebarAlignedLabel>{t.sshKeyManager}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'background-sessions' ? 'active' : ''}`}
            onClick={() => selectTab('background-sessions')}
            aria-label={t.backgroundSessions}
            title={t.backgroundSessions}
            type="button"
          >
            <AppIcon name="history" size={18} />
            <SidebarAlignedLabel>{t.backgroundSessions}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'mcp-call-log' ? 'active' : ''}`}
            onClick={() => selectTab('mcp-call-log')}
            aria-label={t.mcpCallRecordsTitle}
            title={t.mcpCallRecordsTitle}
            type="button"
          >
            <AppIcon name="log" size={18} />
            <SidebarAlignedLabel>{t.mcpCallRecordsTitle}</SidebarAlignedLabel>
          </button>
          <button
            className="sidebar-nav-link"
            onClick={onOpenFileTransfers}
            aria-label={t.fileTransferWorkspace}
            title={t.fileTransferWorkspace}
            type="button"
          >
            <AppIcon name="transfer" size={18} />
            <SidebarAlignedLabel>{t.fileTransferWorkspace}</SidebarAlignedLabel>
          </button>
          <button
            className={`sidebar-nav-link ${currentPage === 'settings' ? 'active' : ''}`}
            onClick={() => selectTab('settings')}
            aria-label={t.settings}
            title={t.settings}
            type="button"
          >
            <MaterialIcon name="settings" className="material-symbols-outlined" />
            <SidebarAlignedLabel>{t.settings}</SidebarAlignedLabel>
          </button>
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
            <MaterialIcon name="star" className="material-symbols-outlined" />
            <SidebarAlignedLabel>GitHub</SidebarAlignedLabel>
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
              <MaterialIcon name={updateHint.icon} aria-hidden="true" className="material-symbols-outlined" />
              <span>{updateHint.label}</span>
            </button>
          ) : null}
          <FeedbackText message={externalUrlError} tone="error" />
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
        <div
          className={`home-content-body scrollbar-scroll ${suppressEntrance && !isServerManagementMode ? 'suppress-home-entrance' : ''} ${isServerManagementMode && managementPage === 'overview' ? 'is-server-management-overview' : ''}`}
          ref={contentBodyRef}
        >
          {currentPage === 'overview' && (
            <div key={pageEntryKey('overview')} className="page-transition" data-nav-direction={navDirection}>
              {isServerManagementMode ? (
                <ServerManagementOverview
                  commandFolders={commandFolders}
                  commandTemplates={commandTemplates}
                  profiles={profiles}
                  maskConnectionAddresses={maskConnectionAddresses}
                  folders={folders}
                  sessions={sessions}
                  workspaceTabs={managementTabs}
                  focusedProfileId={focusedProfileId}
                  focusedSessionTabId={lastActiveSessionTab?.id ?? null}
                  focusRequest={overviewFocusRequest}
                  isFocusMode={tabBarProps.isWorkspaceFocusMode}
                  onToggleFocusMode={tabBarProps.onToggleWorkspaceFocus}
                  selectedFolderId={serverManagement?.selectedFolderId ?? null}
                  onSelectFolder={(folderId) => serverManagement?.navigate('overview', folderId)}
                  onConnectProfile={onConnectProfile}
                  onDisconnectSession={onDisconnectSession}
                  onActivateSession={tabBarProps.onActivateSession}
                  onCreateConnection={onCreateConnection}
                  onEditConnection={onEditConnection}
                  onUpdateConnectionOrder={onUpdateConnectionOrder}
                />
              ) : (
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
              )}
            </div>
          )}
          {currentPage === 'server-topology' && (
            <div key={pageEntryKey('server-topology')} className="page-transition" data-nav-direction={navDirection}>
              <ServerTopologyPage
                profiles={profiles}
                sessions={sessions}
                workspaceTabs={managementTabs}
                focusedSessionTabId={lastActiveSessionTab?.id ?? null}
                onConnectProfile={onConnectProfile}
                onDisconnectSession={onDisconnectSession}
                onActivateSession={tabBarProps.onActivateSession}
                onEditConnection={onEditConnection}
                maskConnectionAddresses={maskConnectionAddresses}
                topologyServiceProbeIntervalSeconds={uiPreferences?.topologyServiceProbeIntervalSeconds}
              />
            </div>
          )}
          {currentPage === 'connection-manager' && (
            <div key={pageEntryKey('connection-manager')} className="page-transition" data-nav-direction={navDirection}>
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
          {currentPage === 'proxy-manager' && (
            <div key={pageEntryKey('proxy-manager')} className="page-transition" data-nav-direction={navDirection}>
              <ProxyManagerPage onStatsChange={setProxyStats} onActiveFilterChange={setActiveProxyFilterLabel} />
            </div>
          )}
          {currentPage === 'command-manager' && (
            <div key={pageEntryKey('command-manager')} className="page-transition" data-nav-direction={navDirection}>
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
          {currentPage === 'ssh-key-manager' && (
            <div key={pageEntryKey('ssh-key-manager')} className="page-transition" data-nav-direction={navDirection}>
              <SshKeyManagerPage onActiveFolderChange={setActiveSshKeyFolderName} onStatsChange={setSshKeyStats} />
            </div>
          )}
          {currentPage === 'background-sessions' && (
            <div
              key={pageEntryKey('background-sessions')}
              className="page-transition"
              data-nav-direction={navDirection}
            >
              <BackgroundSessionsPage
                profiles={profiles}
                tabs={backgroundTabs}
                sessions={sessions}
                onAttach={onAttachBackgroundSession}
                onClose={onCloseBackgroundSession}
              />
            </div>
          )}
          {currentPage === 'mcp-call-log' && (
            <div key={pageEntryKey('mcp-call-log')} className="page-transition" data-nav-direction={navDirection}>
              <McpCallLogPage onTotalChange={setMcpCallCount} />
            </div>
          )}
          {currentPage === 'settings' && (
            <div key={pageEntryKey('settings')} className="page-transition" data-nav-direction={navDirection}>
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
                workspaceMode={isServerManagementMode ? 'server-management' : 'standard'}
                uiPreferences={uiPreferences}
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
            {currentPage === 'connection-manager' && (
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
            {currentPage === 'proxy-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>
                  {proxyStats.totalCount} {t.allProxiesAndTunnels}
                </span>
                <span className="footer-meta-separator">|</span>
                <span>
                  {proxyStats.proxyCount} {t.proxyType}
                </span>
                <span className="footer-meta-separator">|</span>
                <span>
                  {proxyStats.tunnelCount} {t.tunnelProfileType}
                </span>
                {activeProxyFilterLabel && (
                  <>
                    <span className="footer-meta-separator">|</span>
                    <span>{activeProxyFilterLabel}</span>
                  </>
                )}
              </>
            )}
            {currentPage === 'command-manager' && (
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
            {currentPage === 'ssh-key-manager' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>{t.keyCount.replace('{count}', String(sshKeyStats.keyCount))}</span>
                <span className="footer-meta-separator">|</span>
                <span>{t.folderCount.replace('{count}', String(sshKeyStats.folderCount))}</span>
                <span className="footer-meta-spacer"></span>
                <span>{activeSshKeyFolderName}</span>
              </>
            )}
            {currentPage === 'mcp-call-log' && (
              <>
                <span className="footer-meta-separator">|</span>
                <span>{t.mcpCallRecordsCount.replace('{count}', String(mcpCallCount))}</span>
              </>
            )}
          </div>
          {/* Footer nav hidden: Changelog, API Reference, Status — code/handlers preserved */}
        </footer>
      </main>
      {isPortableUpdateDialogOpen ? (
        <PortableUpdateDialog
          onClose={() => setPortableUpdateDialogOpen(false)}
          onOpenReleasePage={() => {
            if (import.meta.env.DEV && !updateStatus?.isPortable) return
            void openExternalUrl(updateStatus?.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
          }}
        />
      ) : null}
    </section>
  )
}
