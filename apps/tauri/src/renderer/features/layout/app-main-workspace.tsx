import './app-main-workspace.css'
import { useEffect, useState, type CSSProperties, type MouseEvent } from 'react'
import { DEFAULT_UI_ZOOM_PERCENT, MAX_UI_ZOOM_PERCENT, MIN_UI_ZOOM_PERCENT, UI_ZOOM_PERCENT_STEP } from '@fileterm/core'
import { AiCopilotPanel } from '../ai/ai-copilot-panel'
import { CloseButton } from '../common/close-button'
import { SystemSidebarShell } from '../system/system-sidebar-shell'
import { resolveMonitoringSource } from '../system/shared-monitoring'
import { TransferCenterHost } from '../transfers/transfer-center-host'
import { KeepAliveWorkspaceStage } from '../workspace/workspace-stage'
import { TabBar, type TabBarProps, type TabContextTarget } from './tab-bar'
import type { AppViewModel } from './app-view-model'
import { WindowMenubar } from './window-menubar'
import { HostRail } from './host-rail'
import { useHostNavigation } from './use-host-navigation'
import { useWorkspaceTabShortcuts } from './use-workspace-tab-shortcuts'
import { DEFAULT_SIDEBAR_WIDTH, STATUS_MESSAGE_TIMEOUT_MS } from '../../app/app-shell-utils'
import { t, setLocale } from '../../i18n'
import { resolveSshJumpRoute } from '../../app/ssh-jump-route'

export function AppMainWorkspace({ model }: { model: AppViewModel }) {
  const [isHostRailCollapsed, setIsHostRailCollapsed] = useState(false)
  const [dismissedInteractiveGatewayNoticeTabId, setDismissedInteractiveGatewayNoticeTabId] = useState<string | null>(
    null
  )
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
    uiZoomLocked,
    setUiZoomLocked,
    setUiZoomPercent,
    filePanelRememberRatio,
    sidebarWidth,
    setSidebarWidth,
    tabBarLayout,
    compactTabBar,
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
    openFileTransfers,
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
    activeSidebarWidth,
    setActiveSidebarWidth,
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
    setProfileFilePaneMode,
    createConnectionFolder,
    deleteConnectionFolder,
    updateConnectionFolder,
    updateConnectionOrder,
    openLogsDirectory
  } = data
  const { startSidebarResize, startHostRailResize, isResizingHostRail, startAiCopilotResize } = resize

  const isInteractiveGatewayRoutePending =
    activeSession?.resourceMonitoringUnavailableReason === 'interactive-gateway-target-route-required'
  const interactiveGatewayNoticeTabId = isInteractiveGatewayRoutePending ? (activeTab?.id ?? null) : null
  const isInteractiveGatewayNoticeVisible =
    interactiveGatewayNoticeTabId !== null && dismissedInteractiveGatewayNoticeTabId !== interactiveGatewayNoticeTabId

  useEffect(() => {
    if (!interactiveGatewayNoticeTabId || !isInteractiveGatewayNoticeVisible) {
      return
    }
    const timeoutId = window.setTimeout(() => {
      setDismissedInteractiveGatewayNoticeTabId((current) =>
        current === interactiveGatewayNoticeTabId ? current : interactiveGatewayNoticeTabId
      )
    }, STATUS_MESSAGE_TIMEOUT_MS)
    return () => window.clearTimeout(timeoutId)
  }, [interactiveGatewayNoticeTabId, isInteractiveGatewayNoticeVisible])

  useEffect(() => {
    if (interactiveGatewayNoticeTabId || !activeTab?.id) {
      return
    }
    setDismissedInteractiveGatewayNoticeTabId((current) => (current === activeTab.id ? null : current))
  }, [activeTab?.id, interactiveGatewayNoticeTabId])

  const hasStatusMessage = Boolean(error || isInteractiveGatewayNoticeVisible)
  const usesHostGroups = tabBarLayout === 'host-groups'
  const resolvedSidebarWidth = isSystemSidebarCollapsed
    ? 44
    : usesHostGroups && !isHomeWorkspaceVisible
      ? activeSidebarWidth
      : resize.expandedSidebarWidth
  const showWorkspaceLeftFrame = !usesHostGroups && (isLocalTerminalWorkspace || activeLocalTab?.kind === 'transfers')
  const showTerminalRightFrame = !isHomeWorkspaceVisible && !usesHostGroups && !shouldShowAiCopilot
  const activeJumpHosts =
    activeProfile?.type === 'ssh' ? resolveSshJumpRoute(activeProfile, workspace.profiles).jumpHosts : []
  const activeRouteTarget = activeJumpHosts.length && activeProfile?.type === 'ssh' ? activeProfile : undefined
  const showHostRail = usesHostGroups && !isHomeWorkspaceVisible
  const activateSession = (tabId: string) => {
    void activateSessionTab(tabId)
  }
  const hostNavigation = useHostNavigation({
    orderedTabs,
    profiles: workspace.profiles,
    activeHomeTabId: effectiveActiveLocalTabId,
    activeSessionTabId: visibleActiveSessionTabId,
    onActivateHome: activateHomeTab,
    onActivateSession: activateSession,
    onAddHomeTab: addHomeTab
  })
  useWorkspaceTabShortcuts({
    tabs: usesHostGroups ? hostNavigation.activeEntries : orderedTabs,
    activeKey: effectiveActiveLocalTabId
      ? `home:${effectiveActiveLocalTabId}`
      : visibleActiveSessionTabId
        ? `session:${visibleActiveSessionTabId}`
        : null,
    onActivateHome: activateHomeTab,
    onActivateSession: activateSession
  })
  // Sessions on one machine share a collector, so machine info shows the collecting tab's samples.
  const monitoringSource = resolveMonitoringSource(activeTab?.id ?? null, workspace.sessions)
  const activeHostGroup = hostNavigation.groups.find((group) => group.key === hostNavigation.activeGroupKey)
  // "+" opens another tab on the active machine.
  const addTabToActiveHost = () => {
    if (activeHostGroup?.kind === 'local') {
      void openLocalTerminal()
    } else if (activeHostGroup?.kind === 'profile' && activeHostGroup.profileId) {
      void openProfile(activeHostGroup.profileId)
    } else {
      addHomeTab()
    }
  }
  const tabBarProps: Omit<TabBarProps, 'homeBrandContent'> = {
    activeHomeTabId: effectiveActiveLocalTabId,
    activeSessionTabId: visibleActiveSessionTabId,
    isAiCopilotAvailable,
    isAiCopilotOpen,
    isWorkspaceFocusMode,
    onAddHomeTab: usesHostGroups ? addTabToActiveHost : addHomeTab,
    onActivateHome: activateHomeTab,
    onActivateSession: activateSession,
    onCloseHomeTab: closeHomeTab,
    onCloseSessionTab: (event: MouseEvent<HTMLElement>, tabId: string) => {
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
      if (!nextFocusMode) setActiveSidebarWidth(DEFAULT_SIDEBAR_WIDTH)
    },
    orderedTabs,
    hostNavigation: usesHostGroups ? hostNavigation : undefined
  }

  return (
    <>
      <div
        className={`fs-shell ${usesCustomWindowChrome ? 'has-window-menubar' : ''} ${compactTabBar ? 'is-compact-tabbar' : ''} ${isMaximized ? 'is-window-maximized' : ''} ${isHomeWorkspaceVisible ? 'is-home-active' : ''} ${isLocalTerminalWorkspace ? 'is-local-terminal' : ''} ${isSystemSidebarCollapsed ? 'is-sidebar-collapsed' : ''} ${showHostRail ? 'has-host-rail' : ''} ${showWorkspaceLeftFrame ? 'has-workspace-left-frame' : ''} ${shouldShowSystemSidebar ? 'has-system-sidebar' : ''} ${isResizingSidebar || isResizingHostRail ? 'is-resizing-sidebar' : ''} ${isResizingAiCopilot ? 'is-resizing-copilot' : ''} ${shouldShowAiCopilot ? 'has-ai-copilot' : ''} ${usesCustomWindowChrome && sessionSecurity.isLocked ? 'is-session-locked' : ''}`}
        style={
          {
            '--sidebar-width': `${resolvedSidebarWidth}px`,
            '--brand-width': `${DEFAULT_SIDEBAR_WIDTH}px`,
            '--host-rail-width': `${isHostRailCollapsed ? 44 : sidebarWidth}px`,
            '--ai-copilot-panel-width': `${aiCopilotWidth}px`
          } as CSSProperties
        }
      >
        {usesCustomWindowChrome ? (
          <WindowMenubar
            desktopApi={desktopApi}
            isMaximized={isMaximized}
            terminalZoomLocked={terminalZoomLocked}
            uiZoomLocked={uiZoomLocked}
            onUiZoomPercentChange={(operation) => {
              if (uiZoomLocked) return
              setUiZoomPercent((current) => {
                if (operation === 'reset') return DEFAULT_UI_ZOOM_PERCENT
                return Math.max(
                  MIN_UI_ZOOM_PERCENT,
                  Math.min(
                    MAX_UI_ZOOM_PERCENT,
                    current + (operation === 'in' ? UI_ZOOM_PERCENT_STEP : -UI_ZOOM_PERCENT_STEP)
                  )
                )
              })
            }}
            onToggleUiZoomLock={() => setUiZoomLocked((current) => !current)}
            onToggleTerminalZoomLock={() => setTerminalZoomLocked((current) => !current)}
          />
        ) : null}
        {!isHomeWorkspaceVisible && <TabBar {...tabBarProps} />}

        {showHostRail ? (
          <HostRail
            navigation={hostNavigation}
            collapsed={isHostRailCollapsed}
            width={sidebarWidth}
            isResizing={isResizingHostRail}
            onToggleCollapsed={() => setIsHostRailCollapsed((current) => !current)}
            onResizeStart={startHostRailResize}
            onWidthChange={setSidebarWidth}
          />
        ) : null}

        {shouldShowSystemSidebar ? (
          <SystemSidebarShell
            activeProfile={activeProfile}
            activeSession={monitoringSource.session}
            activeTabId={monitoringSource.tabId}
            monitoringPeers={monitoringSource.peers}
            collapsed={isSystemSidebarCollapsed}
            connectionStatus={activeTab?.status ?? null}
            showResourceMeters={isResourceMonitoringAvailable}
            visibleMetrics={activeSidebarMetricOrder.filter((metric) => activeSidebarMetrics.includes(metric))}
            isResizing={isResizingSidebar}
            onMonitoringSnapshot={shell.applySnapshot}
            onOpenSystemInfo={openSystemInfo}
            onResizeStart={startSidebarResize}
            onRestoreWidth={() => setActiveSidebarWidth(DEFAULT_SIDEBAR_WIDTH)}
            onToggleCollapsed={setIsSystemSidebarCollapsed}
          />
        ) : null}

        <main
          className={`fs-main ${hasStatusMessage ? 'has-status' : 'no-status'} ${shouldShowSystemSidebar ? '' : 'full-width'}`}
        >
          {error ? (
            <div className="status-message" role="alert">
              <span className="status-message-text">{error}</span>
              <CloseButton
                className="status-message-close"
                aria-label={t.closeTab}
                onClick={() => setError(null)}
                size="compact"
              />
            </div>
          ) : isInteractiveGatewayNoticeVisible ? (
            <div className="status-message" role="status" aria-live="polite">
              <span className="status-message-text">
                <strong>{t.interactiveGatewayResourceMonitoringTitle}</strong>{' '}
                {t.interactiveGatewayResourceMonitoringDescription}
              </span>
              <CloseButton
                className="status-message-close"
                aria-label={t.closeTab}
                size="compact"
                onClick={() => {
                  if (interactiveGatewayNoticeTabId) {
                    setDismissedInteractiveGatewayNoticeTabId(interactiveGatewayNoticeTabId)
                  }
                }}
              />
            </div>
          ) : null}
          <div className={`workspace-stage ${shouldShowAiCopilot ? 'has-ai-copilot' : ''}`}>
            <div
              className={`workspace-stage-transition ${isWorkspaceTransitionActive ? 'is-transitioning' : ''}`}
              data-nav-direction={workspaceNavDirection}
            >
              <KeepAliveWorkspaceStage
                onApplySnapshot={shell.applySnapshot}
                activeLocalTab={activeLocalTab}
                fileTransferTab={workspaceState.localTabs.find((tab) => tab.kind === 'transfers')}
                desktopApi={desktopApi}
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
                onSetProfileFilePaneMode={setProfileFilePaneMode}
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
                onOpenFileTransfers={openFileTransfers}
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

        {/* Machine info borders the terminal in the host-groups layout, so the edge frame would be a stray strip. */}
        <div className="workspace-left-frame" aria-hidden="true" />
        <div className={`terminal-right-frame ${showTerminalRightFrame ? 'is-visible' : ''}`} aria-hidden="true" />

        <TransferCenterHost
          showAll={activeLocalTab?.kind === 'transfers'}
          activeProfileId={activeTab?.profileId}
          activeTabId={activeTab?.id ?? null}
          activeTabStatus={activeTab?.status ?? null}
          activeJumpHosts={activeJumpHosts}
          activeRouteTarget={activeRouteTarget}
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
