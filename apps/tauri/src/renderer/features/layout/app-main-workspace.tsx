import type { CSSProperties, MouseEvent } from 'react'
import { AiCopilotPanel } from '../ai/ai-copilot-panel'
import { CloseButton } from '../common/close-button'
import { SystemSidebarShell } from '../system/system-sidebar-shell'
import { TransferCenterHost } from '../transfers/transfer-center-host'
import { KeepAliveWorkspaceStage } from '../workspace/workspace-stage'
import { TabBar, type TabBarProps, type TabContextTarget } from './tab-bar'
import type { AppViewModel } from './app-view-model'
import { WindowMenubar } from './window-menubar'
import { t, setLocale } from '../../i18n'

export function AppMainWorkspace({ model }: { model: AppViewModel }) {
  const { shell, workspace: workspaceState, data, resize, usesCustomWindowChrome } = model
  const {
    workspace,
    desktopApi,
    error,
    setError,
    isBusy,
    isMaximized,
    themeMode,
    themeConfig,
    customThemes,
    locale,
    setLocaleState,
    terminalZoomLocked,
    setTerminalZoomLocked,
    filePanelRememberRatio,
    sidebarWidth,
    setSidebarWidth,
    aiCopilotWidth,
    isResizingSidebar,
    isResizingAiCopilot,
    isAiCopilotOpen,
    setIsAiCopilotOpen,
    setSettingsInitialTab,
    sessionSecurity
  } = shell
  const {
    isWorkspaceTransitionActive,
    isWorkspaceSwitching,
    visibleWorkspaceTabs,
    backgroundWorkspaceTabs,
    activeLocalTab,
    visibleActiveSessionTabId,
    activeTab,
    addHomeTab,
    isHomeWorkspaceVisible,
    activeSession,
    activeProfile,
    activePaneTab,
    activePaneSession,
    effectiveActiveLocalTabId,
    workspaceNavDirection,
    orderedTabs,
    sessionSendTargets,
    activeTerminalDockSendState,
    updateTerminalDockSendScope,
    updateTerminalDockSelectedTabIds,
    sendTerminalCommand,
    openProfile,
    openLocalTerminal,
    activateSessionTab,
    attachBackgroundSession,
    detachSessionToBackground,
    closeBackgroundSession,
    reconnectSessionTab,
    openTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag,
    activateHomeTab,
    closeHomeTab,
    closeSessionTab,
    openSystemInfo,
    splitPane,
    closePane,
    closeActiveWorkspaceItem,
    activatePane,
    setPaneWeights,
    setIsSystemSidebarCollapsed,
    activeFilePanelHeight,
    activeFilePanelRatio,
    activeFilePanelSnapTarget,
    activeWorkspaceFocusKey,
    isWorkspaceFocusMode,
    activeWorkspaceView,
    activeCommandPaneWidth,
    activeSidebarMetrics,
    activeSidebarMetricOrder,
    isResourceMonitoringAvailable,
    shouldShowSystemSidebar,
    isSystemSidebarCollapsed,
    aiCopilotTargetTab,
    aiCopilotTargetSession,
    isAiCopilotAvailable,
    shouldShowAiCopilot,
    setActiveFilePanelHeight,
    commitActiveFilePanelRatio,
    commitActiveFilePanelSnapTarget,
    setActiveCommandPaneWidth,
    isLocalTerminalWorkspace,
    localPanePath,
    remoteFileAccessMode,
    isRemoteDirectoryLoading,
    isWorkspaceRefreshing,
    canPasteIntoLocal,
    canPasteIntoRemote,
    localCutPaths,
    remoteCutPaths,
    clipboardStatusText,
    copyItems,
    cutItems,
    clearCutState,
    handleChooseUploadFiles,
    handleDownloadFiles,
    handleDownloadLocalNetworkFiles,
    handleDropUpload,
    handleOpenLocalItem,
    handleOpenLocalPath,
    handleBackToLocalComputer,
    handleOpenRemoteItem,
    handleOpenRemotePath,
    handlePasteIntoPane,
    requestChangePermissions,
    requestDelete,
    requestNewFile,
    requestNewFolder,
    handleQuickDelete,
    requestRename,
    handleToggleFollowShellCwd,
    handleToggleRemoteFileAccessMode,
    handleRefreshWorkspace,
    handleUploadFiles,
    launchLocalAgent,
    openCommandManager
  } = workspaceState
  const {
    executeCommandTemplate,
    saveCommandTemplate,
    deleteCommandTemplate,
    createCommandFolder,
    deleteCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    handleDeleteProfile,
    createConnectionFolder,
    deleteConnectionFolder,
    updateConnectionFolder,
    updateConnectionOrder,
    openLogsDirectory
  } = data
  const { startSidebarResize, startAiCopilotResize } = resize

  const resolvedSidebarWidth = isSystemSidebarCollapsed ? 44 : sidebarWidth
  const activeJumpHost =
    activeProfile?.type === 'ssh' && activeProfile.jumpProfileId
      ? (workspace.profiles.find((profile) => profile.id === activeProfile.jumpProfileId) ?? null)
      : null
  // Keep the home titlebar brand independent from the collapsed sidebar. When
  // the sidebar is expanded, its live width still drives the brand column so
  // the two boundaries track together during a resize.
  const brandWidth = isHomeWorkspaceVisible && !isSystemSidebarCollapsed ? sidebarWidth : 214
  const tabBarProps: Omit<TabBarProps, 'homeBrandContent'> = {
    activeHomeTabId: effectiveActiveLocalTabId,
    activeSessionTabId: visibleActiveSessionTabId,
    isAiCopilotAvailable,
    isAiCopilotOpen,
    isWorkspaceFocusMode,
    onAddHomeTab: addHomeTab,
    onActivateHome: activateHomeTab,
    onActivateSession: (tabId: string) => {
      void activateSessionTab(tabId)
    },
    onCloseHomeTab: closeHomeTab,
    onCloseSessionTab: (event: MouseEvent<HTMLButtonElement>, tabId: string) => {
      void closeSessionTab(event, tabId)
    },
    onDragEnd: endTabDrag,
    onDragEnter: enterDraggedTab,
    onDragStart: startTabDrag,
    onOpenSettings: () => {
      setSettingsInitialTab('interface')
      workspaceState.setShowSettings(true)
    },
    onToggleAiCopilot: () => setIsAiCopilotOpen((current) => !current),
    onToggleWindowMaximize: () => {
      void desktopApi?.toggleMaximizeCurrentWindow()
    },
    onOpenTabContext: (event: MouseEvent<HTMLDivElement>, target: TabContextTarget) => {
      openTabContextMenu(event, target)
    },
    onToggleWorkspaceFocus: () => {
      if (!activeWorkspaceFocusKey) return
      const nextFocusMode = !isWorkspaceFocusMode
      shell.setWorkspaceFocusModes((currentModes) => ({
        ...currentModes,
        [activeWorkspaceFocusKey]: nextFocusMode
      }))
      if (!nextFocusMode) setSidebarWidth(214)
    },
    orderedTabs
  }

  return (
    <>
      <div
        className={`fs-shell ${usesCustomWindowChrome ? 'has-window-menubar' : ''} ${isMaximized ? 'is-window-maximized' : ''} ${isHomeWorkspaceVisible ? 'is-home-active' : ''} ${isLocalTerminalWorkspace ? 'is-local-terminal' : ''} ${isSystemSidebarCollapsed ? 'is-sidebar-collapsed' : ''} ${isResizingSidebar ? 'is-resizing-sidebar' : ''} ${isResizingAiCopilot ? 'is-resizing-copilot' : ''} ${shouldShowAiCopilot ? 'has-ai-copilot' : ''} ${usesCustomWindowChrome && sessionSecurity.isLocked ? 'is-session-locked' : ''}`}
        style={
          {
            '--sidebar-width': `${resolvedSidebarWidth}px`,
            '--brand-width': `${brandWidth}px`,
            '--ai-copilot-panel-width': `${aiCopilotWidth}px`
          } as CSSProperties
        }
      >
        {usesCustomWindowChrome ? (
          <WindowMenubar
            desktopApi={desktopApi}
            isMaximized={isMaximized}
            terminalZoomLocked={terminalZoomLocked}
            onToggleTerminalZoomLock={() => setTerminalZoomLocked((current) => !current)}
          />
        ) : null}
        {!isHomeWorkspaceVisible && <TabBar {...tabBarProps} />}

        {shouldShowSystemSidebar ? (
          <SystemSidebarShell
            activeProfile={activeProfile}
            activeSession={activeSession}
            collapsed={isSystemSidebarCollapsed}
            showResourceMeters={isResourceMonitoringAvailable}
            visibleMetrics={activeSidebarMetricOrder.filter((metric) => activeSidebarMetrics.includes(metric))}
            isResizing={isResizingSidebar}
            onOpenSystemInfo={openSystemInfo}
            onResizeStart={startSidebarResize}
            onRestoreWidth={() => setSidebarWidth(214)}
            onToggleCollapsed={setIsSystemSidebarCollapsed}
          />
        ) : null}

        <main
          className={`fs-main ${error ? 'has-status' : 'no-status'} ${shouldShowSystemSidebar ? '' : 'full-width'}`}
        >
          {error ? (
            <div className="status-message" role="alert">
              <span className="status-message-text">{error}</span>
              <CloseButton aria-label={t.closeTab} onClick={() => setError(null)} size="compact" />
            </div>
          ) : null}
          <div className={`workspace-stage ${shouldShowAiCopilot ? 'has-ai-copilot' : ''}`}>
            <div
              className={`workspace-stage-transition ${isWorkspaceTransitionActive ? 'is-transitioning' : ''}`}
              data-nav-direction={workspaceNavDirection}
            >
              <KeepAliveWorkspaceStage
                activeLocalTab={activeLocalTab}
                activeHomeTabId={effectiveActiveLocalTabId}
                activeProfile={activeProfile}
                activeSession={activeSession}
                activeTab={activeTab}
                terminalActiveTab={activePaneTab ?? activeTab}
                terminalActiveSession={activePaneSession ?? activeSession}
                splitRootTab={activeTab?.paneRoot ? activeTab : undefined}
                activeView={activeWorkspaceView}
                commandPaneWidth={activeCommandPaneWidth}
                onCommandPaneWidthChange={setActiveCommandPaneWidth}
                onActiveViewChange={(view) => {
                  if (!activeTab) return
                  shell.setWorkspaceViews((currentViews) => ({ ...currentViews, [activeTab.id]: view }))
                }}
                filePanelHeight={activeFilePanelHeight}
                onFilePanelHeightChange={setActiveFilePanelHeight}
                filePanelRatio={activeFilePanelRatio}
                onFilePanelRatioCommit={commitActiveFilePanelRatio}
                filePanelSnapTarget={activeFilePanelSnapTarget}
                onFilePanelSnapTargetCommit={commitActiveFilePanelSnapTarget}
                rememberFilePanelRatio={filePanelRememberRatio}
                sendTargets={sessionSendTargets}
                terminalDockSendScope={activeTerminalDockSendState.scope}
                terminalDockSelectedTabIds={activeTerminalDockSendState.selectedTabIds}
                commandFolders={workspace.commandFolders || []}
                commandTemplates={workspace.commandTemplates || []}
                folders={workspace.folders || []}
                isBusy={isBusy}
                localItems={shell.localItems}
                localPath={shell.localPath}
                localPanePath={localPanePath}
                isLocalNetworkShare={workspaceState.isLocalNetworkShare}
                isLocalDirectoryLoading={shell.isLocalDirectoryLoading}
                isWorkspaceRefreshing={isWorkspaceRefreshing}
                isWorkspaceSwitching={isWorkspaceSwitching}
                canPasteToLocal={canPasteIntoLocal}
                canPasteToRemote={canPasteIntoRemote}
                clipboardStatusText={clipboardStatusText}
                localCutPaths={localCutPaths}
                remoteCutPaths={remoteCutPaths}
                onCopyItems={copyItems}
                onCutItems={cutItems}
                onClearCutState={clearCutState}
                onExecuteCommand={(commandId, args, options, scope, selectedTabIds) => {
                  void executeCommandTemplate(commandId, args, options, scope, selectedTabIds)
                }}
                onSendTerminalCommand={sendTerminalCommand}
                onSaveTemporaryCommand={(command) => {
                  if (desktopApi) {
                    return desktopApi
                      .openCommandFormWindow('create', undefined, undefined, command)
                      .then(() => true)
                      .catch(() => false)
                  }
                  return false
                }}
                onTerminalDockSendScopeChange={updateTerminalDockSendScope}
                onTerminalDockSelectedTabIdsChange={updateTerminalDockSelectedTabIds}
                onOpenCommandManager={openCommandManager}
                profiles={workspace.profiles}
                backgroundTabs={backgroundWorkspaceTabs}
                onAttachBackgroundSession={attachBackgroundSession}
                onCloseBackgroundSession={closeBackgroundSession}
                onChooseUploadFiles={handleChooseUploadFiles}
                onDownloadFiles={handleDownloadFiles}
                onDownloadLocalNetworkFiles={handleDownloadLocalNetworkFiles}
                onDropUpload={handleDropUpload}
                onOpenLocalItem={handleOpenLocalItem}
                onOpenLocalPath={handleOpenLocalPath}
                onBackToLocalComputer={handleBackToLocalComputer}
                onOpenProfile={openProfile}
                onOpenLocalTerminal={() => void openLocalTerminal()}
                onLaunchLocalAgent={launchLocalAgent}
                onReconnectLocalTerminal={reconnectSessionTab}
                onOpenRemoteItem={handleOpenRemoteItem}
                onOpenRemotePath={handleOpenRemotePath}
                onPasteIntoPane={handlePasteIntoPane}
                onRequestChangePermissions={requestChangePermissions}
                onRequestDelete={requestDelete}
                onRequestNewFile={requestNewFile}
                onRequestNewFolder={requestNewFolder}
                onRequestQuickDelete={handleQuickDelete}
                onRequestRename={requestRename}
                onToggleFollowShellCwd={handleToggleFollowShellCwd}
                onToggleRemoteFileAccessMode={handleToggleRemoteFileAccessMode}
                remoteFileAccessMode={remoteFileAccessMode}
                isRemoteDirectoryLoading={isRemoteDirectoryLoading}
                onRefresh={handleRefreshWorkspace}
                onUploadFiles={handleUploadFiles}
                theme={themeMode}
                themeConfig={themeConfig}
                customThemes={customThemes}
                locale={locale}
                overviewShowStats={shell.overviewShowStats}
                overviewShowRecent={shell.overviewShowRecent}
                overviewShowAllConnections={shell.overviewShowAllConnections}
                overviewShowQuickActions={shell.overviewShowQuickActions}
                overviewSectionOrder={shell.overviewSectionOrder}
                onCreateConnection={() => {
                  if (desktopApi) void desktopApi.openConnectionFormWindow('create')
                }}
                onEditConnection={workspaceState.openEditConnection}
                onDeleteConnection={handleDeleteProfile}
                onCreateConnectionFolder={createConnectionFolder}
                onDeleteConnectionFolder={deleteConnectionFolder}
                onUpdateConnectionFolder={updateConnectionFolder}
                onUpdateConnectionOrder={updateConnectionOrder}
                onImportConnections={shell.openConnectionImportPreview}
                onExportConnections={() => {
                  const request = desktopApi?.exportConnections('fileterm')
                  void request?.then(() => undefined)
                }}
                onCreateCommand={(input) => saveCommandTemplate(null, input)}
                onUpdateCommand={saveCommandTemplate}
                onDeleteCommand={deleteCommandTemplate}
                onCreateCommandFolder={createCommandFolder}
                onDeleteCommandFolder={deleteCommandFolder}
                onUpdateCommandFolder={updateCommandFolder}
                onUpdateCommandOrder={updateCommandOrder}
                onSetTheme={shell.handleSetTheme}
                onSetThemeConfig={shell.setThemeConfig}
                onSetCustomThemes={shell.setCustomThemes}
                onSetLocale={(nextLocale) => {
                  setLocale(nextLocale)
                  setLocaleState(nextLocale)
                }}
                onOpenLogsDirectory={openLogsDirectory}
                isSidebarCollapsed={isSystemSidebarCollapsed}
                isWorkspaceFocusMode={isWorkspaceFocusMode}
                canLockNow={
                  sessionSecurity.status === 'ready' &&
                  sessionSecurity.settings?.hasLockPassword === true &&
                  !sessionSecurity.isLocked
                }
                onLockNow={sessionSecurity.lockNow}
                tabBarProps={tabBarProps}
                isResizingSidebar={isResizingSidebar}
                onResizeStart={startSidebarResize}
                sessions={workspace.sessions}
                workspaceTabs={visibleWorkspaceTabs}
                filePanelHeights={shell.filePanelHeights}
                filePanelRatios={shell.filePanelRatios}
                filePanelSnapTargets={shell.filePanelSnapTargets}
                commandPaneWidths={shell.commandPaneWidths}
                workspaceFocusModes={shell.workspaceFocusModes}
                workspaceViews={shell.workspaceViews}
                activePaneTabId={activePaneTab?.id}
                onClosePane={closePane}
                onCloseTab={closeActiveWorkspaceItem}
                onSplitPane={splitPane}
                onActivatePane={activatePane}
                onSetPaneWeights={setPaneWeights}
              />
            </div>
            {shouldShowAiCopilot ? (
              <AiCopilotPanel
                activeSession={aiCopilotTargetSession}
                activeTab={aiCopilotTargetTab ?? null}
                rootTab={activeTab ?? null}
                isResizing={isResizingAiCopilot}
                onClose={() => setIsAiCopilotOpen(false)}
                onOpenSettings={() => {
                  setSettingsInitialTab('ai')
                  workspaceState.setShowSettings(true)
                }}
                onResizeStart={startAiCopilotResize}
              />
            ) : null}
          </div>
        </main>

        <TransferCenterHost
          activeProfileId={activeTab?.profileId}
          activeTabId={activeTab?.id ?? null}
          activeTabStatus={activeTab?.status ?? null}
          activeJumpHost={
            activeJumpHost ? { name: activeJumpHost.name, host: activeJumpHost.host, port: activeJumpHost.port } : null
          }
          activeTabSource={activeTab?.source ?? null}
          desktopApi={desktopApi}
          fullWidth={!shouldShowSystemSidebar}
          isPending={isBusy}
          onHideToBackground={detachSessionToBackground}
          onApplySnapshot={shell.applySnapshot}
          onError={shell.reportError}
          sessionTabs={visibleWorkspaceTabs.filter((tab) => tab.sessionType !== 'local')}
          transfers={workspace.transfers}
          visible={!isHomeWorkspaceVisible && !isLocalTerminalWorkspace}
        />
      </div>
    </>
  )
}
