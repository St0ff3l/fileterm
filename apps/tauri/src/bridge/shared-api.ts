import type {
  ActionApprovalRequest,
  AiChatRequest,
  AiContextPreview,
  AiConversation,
  AiConversationSummary,
  AiCopilotModeState,
  AiModelInfo,
  AiProviderSummary,
  AiProviderTestResult,
  AiStreamEvent,
  BackupDownloadMode,
  BackupPasswordRequest,
  BackupUploadMode,
  CommandBatchRun,
  CommandBatchTarget,
  CommandExecutionResult,
  CommandSendPreferences,
  ConnectionImportOptions,
  ConnectionImportResult,
  CreateAiContextPreviewInput,
  CreateAiConversationInput,
  CreateProfileInput,
  CreateProxyProfileInput,
  CreateTunnelProfileInput,
  DeleteAiMessageInput,
  FilePaneMode,
  FileTermDesktopApi,
  ImportedFont,
  ImportSshKeyInput,
  ListAiModelsInput,
  LocalFileItem,
  LocalNetworkShareConnectionResult,
  LocalTerminalLaunchOptions,
  LocalTerminalShellOption,
  McpAgentSetup,
  McpApprovalRequest,
  McpCallLogPage,
  NetworkConnectionsSnapshot,
  PathBookmark,
  PermissionChangeOptions,
  ProxyProfile,
  RemoteExecCredentials,
  RemoteFileAccessOptions,
  RemoteFileItem,
  RemoteFilesUpdate,
  RenameAiConversationInput,
  RetryAiChatInput,
  S3BackupConfig,
  S3BackupConfigInput,
  S3BackupResult,
  SaveAiProviderInput,
  SecuritySettings,
  SecuritySettingsInput,
  SerialControlAction,
  SerialLineStatus,
  SerialPortInfo,
  SerialTransferDirection,
  SerialTransferMode,
  SerialTransferProgress,
  SerialTransferResult,
  SessionMetricsUpdate,
  SetAiContextAttachInput,
  SetAiCopilotModeInput,
  SetAiDangerousCommandRestrictionsInput,
  SshConfigHostSummary,
  SshForwardRule,
  SshInteractionRequest,
  SshInteractionResponse,
  SshKeyImportResult,
  SshKeyMetadata,
  SshNetworkInterfaceInfo,
  SshProxyAssignmentInput,
  SshRouteActivity,
  SshRoutePolicySnapshot,
  SshTunnelSnapshot,
  StartAiChatInput,
  SudoPasswordRequest,
  SummarizeAiConversationTitleInput,
  TerminalCommandHistoryEntry,
  TerminalStatePayload,
  TestAiProviderInput,
  TopologyTcpProbeResult,
  TopologyTcpProbeTarget,
  TransferTask,
  TransferTargetOptions,
  TunnelProfile,
  UiPreferences,
  UiPreferencesInput,
  UpdateProxyProfileInput,
  UpdateTunnelProfileInput,
  WebDavSyncConfig,
  WebDavSyncResult,
  WorkspaceSnapshot
} from '@fileterm/core'
import type { BridgeMetadata, BridgeTransport } from './transport'

export type DesktopOnlyApiKeys =
  | 'getUpdateStatus'
  | 'checkForUpdates'
  | 'downloadUpdate'
  | 'installUpdate'
  | 'onUpdateStatus'
  | 'readClipboardText'
  | 'writeClipboardText'
  | 'openConnectionManagerWindow'
  | 'openCommandManagerWindow'
  | 'openConnectionFormWindow'
  | 'openCommandFormWindow'
  | 'openProxyFormWindow'
  | 'openTunnelFormWindow'
  | 'openFileEditorWindow'
  | 'openExternalUrl'
  | 'openLogsDirectory'
  | 'saveSessionLog'
  | 'minimizeCurrentWindow'
  | 'showCurrentWindow'
  | 'isCurrentWindowMaximized'
  | 'toggleMaximizeCurrentWindow'
  | 'closeCurrentWindow'
  | 'confirmCloseCurrentFileEditor'
  | 'cancelCloseCurrentFileEditor'
  | 'showWindowMenu'
  | 'reloadCurrentWindow'
  | 'toggleDevtools'
  | 'requestCloseCurrentWindow'
  | 'requestQuitApp'
  | 'confirmCloseWindow'
  | 'onWindowMaximizedChange'
  | 'onFileEditorCloseRequest'
  | 'onTerminalData'
  | 'selectLocalFiles'
  | 'selectLocalDirectory'
  | 'importFont'
  | 'selectSshKeyFile'
  | 'previewConnectionImport'
  | 'exportConnections'
  | 'exportConnectionsAsFiles'
  | 'getDroppedFilePaths'
  | 'onWindowCloseRequest'
  | 'onRequestCloseActiveWorkspaceItem'
  | 'onNewTabRequest'
  | 'onSplitPaneRequest'
  | 'onFocusPaneRequest'
  | 'onTerminalZoomRequest'
  | 'onTerminalGestureZoomRequest'

export type PlatformApiOverrides = Pick<FileTermDesktopApi, DesktopOnlyApiKeys>

export function createSharedDesktopApi(
  transport: BridgeTransport,
  metadata: BridgeMetadata,
  overrides: PlatformApiOverrides
): FileTermDesktopApi {
  const securitySettingsListeners = new Set<(settings: SecuritySettings) => void>()

  const notifyLocalSecuritySettingsListeners = (settings: SecuritySettings) => {
    for (const listener of [...securitySettingsListeners]) {
      try {
        listener(settings)
      } catch (error) {
        console.error('Failed to apply local security settings update:', error)
      }
    }
  }

  const subscribeSecuritySettings = (listener: (settings: SecuritySettings) => void) => {
    securitySettingsListeners.add(listener)
    const unlistenNative = transport.subscribe('app:security-settings-changed', listener)
    return () => {
      securitySettingsListeners.delete(listener)
      unlistenNative()
    }
  }

  const { invoke, invokeWithChannel, subscribe, subscribeReady } = transport

  return {
    ...metadata,
    ...overrides,
    getPathBookmarks: (scope: string) => invoke<PathBookmark[]>('app_get_path_bookmarks', { scope }),
    updatePathBookmark: (scope, action, bookmark) =>
      invoke<PathBookmark[]>('app_update_path_bookmark', { scope, action, bookmark }),
    getUiPreferences: () => invoke<UiPreferences>('app_get_ui_preferences'),
    setUiPreferences: (input: UiPreferencesInput) => invoke<UiPreferences>('app_set_ui_preferences', { input }),
    getSecuritySettings: () => invoke<SecuritySettings>('app_get_security_settings'),
    setSecuritySettings: async (input: SecuritySettingsInput) => {
      const settings = await invoke<SecuritySettings>('app_set_security_settings', { input })
      notifyLocalSecuritySettingsListeners(settings)
      return settings
    },
    verifySecurityPassword: (password: string) => invoke<boolean>('app_verify_security_password', { password }),
    resetSecurityBackupPassword: async () => {
      const settings = await invoke<SecuritySettings>('app_reset_security_backup_password')
      notifyLocalSecuritySettingsListeners(settings)
      return settings
    },
    listLocalTerminalShells: () => invoke<LocalTerminalShellOption[]>('app_list_local_terminal_shells'),
    getMcpAgentSetup: () => invoke<McpAgentSetup>('app_get_mcp_agent_setup'),
    readMcpCallLog: (limit?: number, offset?: number, before?: string) =>
      invoke<McpCallLogPage>('app_read_mcp_call_log', { limit, offset, before }),
    listAiProviders: () => invoke<AiProviderSummary[]>('app_list_ai_providers'),
    listAiModels: (input: ListAiModelsInput) => invoke<AiModelInfo[]>('app_list_ai_models', { input }),
    saveAiProvider: (input: SaveAiProviderInput) => invoke<AiProviderSummary>('app_save_ai_provider', { input }),
    deleteAiProvider: (providerId: string) => invoke<AiProviderSummary[]>('app_delete_ai_provider', { providerId }),
    testAiProvider: (input: TestAiProviderInput) => invoke<AiProviderTestResult>('app_test_ai_provider', { input }),
    listAiConversations: () => invoke<AiConversationSummary[]>('app_list_ai_conversations'),
    getAiConversation: (conversationId: string) =>
      invoke<AiConversation>('app_get_ai_conversation', { conversationId }),
    createAiConversation: (input: CreateAiConversationInput) =>
      invoke<AiConversation>('app_create_ai_conversation', { input }),
    renameAiConversation: (input: RenameAiConversationInput) =>
      invoke<AiConversation>('app_rename_ai_conversation', { input }),
    summarizeAiConversationTitle: (input: SummarizeAiConversationTitleInput) =>
      invoke<AiConversation>('app_summarize_ai_conversation_title', { input }),
    deleteAiMessage: (input: DeleteAiMessageInput) => invoke<AiConversation>('app_delete_ai_message', { input }),
    deleteAiConversation: (conversationId: string) => invoke<void>('app_delete_ai_conversation', { conversationId }),
    getAiCopilotModeState: () => invoke<AiCopilotModeState>('app_get_ai_copilot_mode_state'),
    setAiCopilotMode: (input: SetAiCopilotModeInput) =>
      invoke<AiCopilotModeState>('app_set_ai_copilot_mode', { input }),
    setAiContextAttach: (input: SetAiContextAttachInput) =>
      invoke<AiCopilotModeState>('app_set_ai_context_attach', { input }),
    setAiDangerousCommandRestrictions: (input: SetAiDangerousCommandRestrictionsInput) =>
      invoke<AiCopilotModeState>('app_set_ai_dangerous_command_restrictions', { input }),
    createAiContextPreview: (input: CreateAiContextPreviewInput) =>
      invoke<AiContextPreview>('app_create_ai_context_preview', { input }),
    startAiChat: (input: StartAiChatInput, onEvent: (event: AiStreamEvent) => void) =>
      invokeWithChannel<AiChatRequest, AiStreamEvent>('app_start_ai_chat', { input }, onEvent),
    retryAiChat: (input: RetryAiChatInput, onEvent: (event: AiStreamEvent) => void) =>
      invokeWithChannel<AiChatRequest, AiStreamEvent>('app_retry_ai_chat', { input }, onEvent),
    cancelAiChat: (requestId: string) => invoke<void>('app_cancel_ai_chat', { requestId }),
    getUiStateItem: (key: string) => invoke<string | null>('app_get_ui_state_item', { key }),
    setUiStateItem: (key: string, value: string) => invoke<void>('app_set_ui_state_item', { key, value }),
    removeUiStateItem: (key: string) => invoke<void>('app_remove_ui_state_item', { key }),
    writeDiagnosticLog: (level: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR', scope: string, message: string) =>
      invoke<void>('app_write_diagnostic_log', { level, scope, message }),
    listSerialPorts: () => invoke<SerialPortInfo[]>('app_list_serial_ports'),
    listSshNetworkInterfaces: () => invoke<SshNetworkInterfaceInfo[]>('app_list_ssh_network_interfaces'),
    getSshRoutePolicy: () => invoke<SshRoutePolicySnapshot>('app_get_ssh_route_policy'),
    saveSshRoutePolicy: (expected, policy) =>
      invoke<SshRoutePolicySnapshot>('app_save_ssh_route_policy', { expected, policy }),
    probeSshRoutePolicy: (profileId) => invoke<SshRoutePolicySnapshot>('app_probe_ssh_route_policy', { profileId }),
    onSshRouteActivity: (listener) => subscribe<SshRouteActivity>('ssh-route:activity', listener),
    onSshRoutePolicyChanged: (listener) => subscribe<null>('ssh-route:policy-changed', listener),
    onSerialPortsChanged: (listener: (ports: SerialPortInfo[]) => void) => subscribe('serial:ports-changed', listener),
    serialControl: (tabId: string, action: SerialControlAction, value?: boolean, durationMs?: number) =>
      invoke<SerialLineStatus>('app_serial_control', { tabId, action, value, durationMs }),
    serialTransfer: (
      tabId: string,
      direction: SerialTransferDirection,
      mode: SerialTransferMode,
      localPath: string,
      fileName?: string,
      localPaths?: string[],
      xmodemPreservePadding?: boolean
    ) =>
      invoke<SerialTransferResult>('app_serial_transfer', {
        tabId,
        direction,
        mode,
        localPath,
        fileName: fileName ?? null,
        localPaths: localPaths ?? null,
        xmodemPreservePadding: xmodemPreservePadding ?? null
      }),
    serialTransferCancel: (tabId: string) => invoke<void>('app_serial_cancel_transfer', { tabId }),
    browseTransferDirectory: (tabId: string, path: string) =>
      invoke<RemoteFileItem[]>('app_browse_transfer_directory', { tabId, path }),
    copyBetweenServers: (
      sourceTabId: string,
      targetTabId: string,
      sourcePath: string,
      targetDirectory: string,
      replaceExisting: boolean
    ) =>
      invoke<string>('app_copy_between_servers', {
        sourceTabId,
        targetTabId,
        sourcePath,
        targetDirectory,
        replaceExisting
      }),
    listLocalDirectory: (dirPath?: string) =>
      invoke<{ path: string; items: LocalFileItem[] }>('app_list_local_directory', {
        dirPath: dirPath ?? null
      }),
    connectLocalNetworkShare: (path: string, username: string, password: string, share?: string) =>
      invoke<LocalNetworkShareConnectionResult>('app_connect_local_network_share', {
        path,
        username,
        password,
        share: share ?? null
      }),
    readLocalFile: (filePath: string, encoding?: string) =>
      invoke<string>('app_read_local_file', { filePath, encoding: encoding ?? null }),
    writeLocalFile: (filePath: string, content: string, encoding?: string) =>
      invoke<void>('app_write_local_file', { filePath, content, encoding: encoding ?? null }),
    createLocalDirectory: (dirPath: string, name: string) =>
      invoke<void>('app_create_local_directory', { dirPath, name }),
    createLocalFile: (dirPath: string, name: string) => invoke<void>('app_create_local_file', { dirPath, name }),
    copyLocalPath: (sourcePath: string, destinationPath: string) =>
      invoke<void>('app_copy_local_path', { sourcePath, destinationPath }),
    moveLocalPath: (sourcePath: string, destinationPath: string) =>
      invoke<void>('app_move_local_path', { sourcePath, destinationPath }),
    renameLocalPath: (targetPath: string, newName: string) =>
      invoke<void>('app_rename_local_path', { targetPath, newName }),
    deleteLocalPath: (targetPath: string) => invoke<void>('app_delete_local_path', { targetPath }),
    changeLocalPermissions: (targetPath: string, options: PermissionChangeOptions) =>
      invoke<void>('app_change_local_permissions', { targetPath, options }),
    queueUpload: (fileNames: string[]) => invoke<WorkspaceSnapshot>('app_queue_upload', { fileNames }),
    cancelTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_cancel_transfer', { transferId }),
    pauseTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_pause_transfer', { transferId }),
    resumeTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_resume_transfer', { transferId }),
    discardTransfer: (transferId: string, force = false) =>
      invoke<WorkspaceSnapshot>('app_discard_transfer', { transferId, force }),
    clearTransfers: (transferIds: string[]) => invoke<WorkspaceSnapshot>('app_clear_transfers', { transferIds }),
    getTerminalCommandHistory: (profileId: string) =>
      invoke<TerminalCommandHistoryEntry[]>('app_get_terminal_command_history', { profileId }),
    setTerminalCommandHistory: (profileId: string, entries: TerminalCommandHistoryEntry[]) =>
      invoke<void>('app_set_terminal_command_history', { profileId, entries }),
    getCommandSendPreferences: () => invoke<CommandSendPreferences>('app_get_command_send_preferences'),
    setCommandSendPreferences: (preferences: CommandSendPreferences) =>
      invoke<void>('app_set_command_send_preferences', { preferences }),
    uploadFile: (tabId: string, localPath: string, remoteDirectory: string, options?: TransferTargetOptions) =>
      invoke<WorkspaceSnapshot>('app_upload_file', { tabId, localPath, remoteDirectory, options: options ?? null }),
    downloadFile: (tabId: string, remotePath: string, localDirectory: string, options?: TransferTargetOptions) =>
      invoke<WorkspaceSnapshot>('app_download_file', { tabId, remotePath, localDirectory, options: options ?? null }),
    downloadRemotePath: (
      tabId: string,
      remotePath: string,
      targetType: 'file' | 'folder',
      localDirectory: string,
      options?: TransferTargetOptions
    ) =>
      invoke<WorkspaceSnapshot>('app_download_remote_path', {
        tabId,
        remotePath,
        targetType,
        localDirectory,
        options: options ?? null
      }),
    getSnapshot: () => invoke<WorkspaceSnapshot>('app_get_snapshot'),
    getConnectionLibrary: () =>
      invoke<{ profiles: WorkspaceSnapshot['profiles']; folders: WorkspaceSnapshot['folders'] }>(
        'app_get_connection_library'
      ),
    listImportedFonts: () => invoke<ImportedFont[]>('app_list_imported_fonts'),
    getImportedFontData: (fontId: string) => invoke<string | null>('app_get_imported_font_data', { fontId }),
    deleteImportedFont: (fontId: string) => invoke<boolean>('app_delete_imported_font', { fontId }),
    listSshKeys: () => invoke<SshKeyMetadata[]>('app_list_ssh_keys'),
    listSshConfigHosts: () => invoke<SshConfigHostSummary[]>('app_list_ssh_config_hosts'),
    importSshKey: (input?: ImportSshKeyInput) => invoke<SshKeyImportResult | null>('app_import_ssh_key', { input }),
    updateSshKeyNote: (keyId: string, note: string) =>
      invoke<SshKeyMetadata>('app_update_ssh_key_note', { keyId, note }),
    deleteSshKey: (keyId: string) => invoke<void>('app_delete_ssh_key', { keyId }),
    onSshKeysChanged: (listener: (keys: SshKeyMetadata[]) => void) => subscribe('sshKeys:changed', listener),
    listProxyProfiles: () => invoke<ProxyProfile[]>('app_list_proxy_profiles'),
    saveProxyProfile: (input: CreateProxyProfileInput | UpdateProxyProfileInput) =>
      invoke<ProxyProfile>('app_save_proxy_profile', { input }),
    deleteProxyProfile: (id: string) => invoke<void>('app_delete_proxy_profile', { id }),
    testProxyProfile: (input: CreateProxyProfileInput | { id: string }) =>
      invoke<{ success: boolean; latencyMs?: number; error?: string }>('app_test_proxy_profile', { input }),
    onProxiesChanged: (listener: () => void) => subscribe('proxies:changed', listener),
    listTunnelProfiles: () => invoke<TunnelProfile[]>('app_list_tunnel_profiles'),
    saveTunnelProfile: (input: CreateTunnelProfileInput | UpdateTunnelProfileInput) =>
      invoke<TunnelProfile>('app_save_tunnel_profile', { input }),
    deleteTunnelProfile: (id: string) => invoke<void>('app_delete_tunnel_profile', { id }),
    onTunnelsChanged: (listener: () => void) => subscribe('tunnels:changed', listener),
    commitConnectionJsonImport: (planId: string, options: ConnectionImportOptions) =>
      invoke<ConnectionImportResult>('app_commit_connection_json_import', { planId, options }),
    getWebDavSyncConfig: () => invoke<WebDavSyncConfig>('app_get_webdav_sync_config'),
    saveWebDavSyncConfig: (input) => invoke<WebDavSyncConfig>('app_set_webdav_sync_config', { input }),
    testWebDavSync: (input: WebDavSyncConfig & { password?: string }) =>
      invoke<WebDavSyncResult>('app_test_webdav_sync', { input }),
    uploadWebDavSync: (mode: BackupUploadMode) => invoke<WebDavSyncResult>('app_upload_webdav_sync', { mode }),
    downloadWebDavSync: (mode: BackupDownloadMode) => invoke<WebDavSyncResult>('app_download_webdav_sync', { mode }),
    getS3BackupConfig: () => invoke<S3BackupConfig>('app_get_s3_backup_config'),
    saveS3BackupConfig: (input: S3BackupConfigInput) => invoke<S3BackupConfig>('app_set_s3_backup_config', { input }),
    testS3Backup: (input: S3BackupConfigInput) => invoke<S3BackupResult>('app_test_s3_backup', { input }),
    uploadS3Backup: (mode: BackupUploadMode) => invoke<S3BackupResult>('app_upload_s3_backup', { mode }),
    downloadS3Backup: (mode: BackupDownloadMode) => invoke<S3BackupResult>('app_download_s3_backup', { mode }),
    createProfile: (input: unknown) => invoke<WorkspaceSnapshot>('app_create_profile', { input }),
    createFolder: (name: string, parentId?: string) =>
      invoke<WorkspaceSnapshot>('app_workspace_mutation', { operation: 'create-folder', payload: { name, parentId } }),
    createCommandFolder: (name: string, parentId?: string) =>
      invoke<WorkspaceSnapshot>('app_workspace_mutation', {
        operation: 'create-command-folder',
        payload: { name, parentId }
      }),
    createCommandTemplate: (input: unknown) =>
      invoke<WorkspaceSnapshot>('app_workspace_mutation', { operation: 'create-command', payload: { input } }),
    assignSshProxy: (input: SshProxyAssignmentInput) => invoke<boolean>('app_assign_ssh_proxy', { input }),
    updateProfile: (profileId: string, input: unknown) =>
      invoke<WorkspaceSnapshot>('app_update_profile', { profileId, input }),
    setProfileFilePaneMode: (profileId: string, mode: FilePaneMode) =>
      invoke<WorkspaceSnapshot>('app_set_profile_file_pane_mode', { profileId, mode }),
    setProfileDefaultDirectory: (profileId, pane, path) =>
      invoke<WorkspaceSnapshot>('app_set_profile_default_directory', { profileId, pane, path }),
    clearTrustedHostFingerprint: (profileId: string) =>
      invoke<WorkspaceSnapshot>('app_clear_trusted_host_fingerprint', { profileId }),
    testConnection: (input: CreateProfileInput, profileId?: string) =>
      invoke<void>('app_test_connection', { input, profileId: profileId ?? null }),
    deleteProfile: (profileId: string) => invoke<WorkspaceSnapshot>('app_delete_profile', { profileId }),
    updateFolder: (folderId: string, updates: unknown) =>
      invoke<WorkspaceSnapshot>('app_update_folder', { folderId, updates }),
    deleteFolder: (folderId: string) => invoke<WorkspaceSnapshot>('app_delete_folder', { folderId }),
    updateEntityOrder: (id: string, newParentId: string | undefined, newOrder: number) =>
      invoke<WorkspaceSnapshot>('app_update_entity_order', {
        id,
        newParentId: newParentId ?? null,
        newOrder
      }),
    updateCommandFolder: (folderId: string, updates: unknown) =>
      invoke<WorkspaceSnapshot>('app_update_command_folder', { folderId, updates }),
    deleteCommandFolder: (folderId: string) => invoke<WorkspaceSnapshot>('app_delete_command_folder', { folderId }),
    updateCommandOrder: (id: string, newParentId: string | undefined, newOrder: number) =>
      invoke<WorkspaceSnapshot>('app_update_command_order', {
        id,
        newParentId: newParentId ?? null,
        newOrder
      }),
    updateCommandTemplate: (commandId: string, input: unknown) =>
      invoke<WorkspaceSnapshot>('app_update_command_template', { commandId, input }),
    deleteCommandTemplate: (commandId: string) =>
      invoke<WorkspaceSnapshot>('app_delete_command_template', { commandId }),
    executeCommandTemplate: (
      tabId: string,
      commandId: string,
      args: string[] = [],
      options?: { appendCarriageReturn?: boolean }
    ) =>
      invoke<CommandExecutionResult>('app_execute_command_template', {
        tabId,
        commandId,
        args,
        options: options ?? null
      }),
    runCommandBatch: (
      commandId: string,
      expectedTemplate: string,
      args: Record<number, string>,
      targets: CommandBatchTarget[]
    ) => invoke<CommandBatchRun>('app_run_command_batch', { commandId, expectedTemplate, args, targets }),
    listCommandBatches: () => invoke<CommandBatchRun[]>('app_list_command_batches'),
    executeRemoteCommand: (
      tabId: string,
      command: string,
      cwd?: string,
      timeoutMs?: number,
      credentials?: RemoteExecCredentials
    ) =>
      invoke<{
        output: string
        exitCode: number | null
        timedOut: boolean
        outputTruncated: boolean
        rawTerminal: boolean
        inputRequired: boolean
        inputKind?: 'secret' | 'text'
      }>('app_execute_remote_command', {
        tabId,
        command,
        cwd: cwd ?? null,
        timeoutMs: timeoutMs ?? null,
        sudoPassword: credentials?.sudoPassword ?? null,
        suPassword: credentials?.suPassword ?? null,
        saveSudoPassword: credentials?.saveSudoPassword ?? null,
        saveSuPassword: credentials?.saveSuPassword ?? null
      }),
    openProfile: (profileId: string) => invoke<WorkspaceSnapshot>('app_open_profile', { profileId }),
    openProfileBackground: (profileId: string) =>
      invoke<WorkspaceSnapshot>('app_open_profile_background', { profileId }),
    openProfileFromManager: (profileId: string) => invoke<WorkspaceSnapshot>('app_open_profile', { profileId }),
    activateTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_activate_tab', { tabId }),
    attachBackgroundSession: (tabId: string) => invoke<WorkspaceSnapshot>('app_attach_background_session', { tabId }),
    detachSessionToBackground: (tabId: string) =>
      invoke<WorkspaceSnapshot>('app_detach_session_to_background', { tabId }),
    hideSessionToBackground: (tabId: string) => invoke<WorkspaceSnapshot>('app_hide_session_to_background', { tabId }),
    setMonitoringEnabled: (tabId, generation, enabled) =>
      invoke<WorkspaceSnapshot>('app_set_monitoring_enabled', { tabId, generation, enabled }),
    getNetworkConnections: (tabId: string) =>
      invoke<NetworkConnectionsSnapshot>('app_get_network_connections', { tabId }),
    probeTopologyServices: (targets: TopologyTcpProbeTarget[]) =>
      invoke<TopologyTcpProbeResult[]>('app_probe_topology_services', { targets }),
    retryMonitoring: (tabId: string, generation: number) => invoke<void>('app_retry_monitoring', { tabId, generation }),
    reconnectTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_reconnect_tab', { tabId }),
    disconnectTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_disconnect_tab', { tabId }),
    closeTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_close_tab', { tabId }),
    splitTab: (sourceTabId: string, direction: 'row' | 'column') =>
      invoke<WorkspaceSnapshot>('app_split_tab', { sourceTabId, direction }),
    closePane: (rootTabId: string, paneTabId: string) =>
      invoke<WorkspaceSnapshot>('app_close_pane', { rootTabId, paneTabId }),
    setActivePane: (rootTabId: string, paneTabId: string) =>
      invoke<WorkspaceSnapshot>('app_set_active_pane', { rootTabId, paneTabId }),
    setPaneWeights: (rootTabId: string, panePath: number[], weights: number[]) =>
      invoke<WorkspaceSnapshot>('app_set_pane_weights', { rootTabId, panePath, weights }),
    openLocalTerminal: (options?: LocalTerminalLaunchOptions) =>
      invoke<WorkspaceSnapshot>('app_open_local_terminal', { options: options ?? null }),
    writeTerminal: (tabId: string, data: string) => invoke<void>('app_write_terminal', { tabId, data }),
    resizeTerminal: (tabId: string, cols: number, rows: number, width: number, height: number) =>
      invoke<void>('app_resize_terminal', { tabId, cols, rows, width, height }),
    openRemotePath: (tabId: string, targetPath: string) =>
      invoke<WorkspaceSnapshot>('app_open_remote_path', { tabId, targetPath }),
    listRemoteDirectory: (tabId: string, targetPath: string) =>
      invoke<RemoteFileItem[]>('app_list_remote_directory', { tabId, targetPath }),
    setFollowShellCwd: (tabId: string, enabled: boolean) =>
      invoke<WorkspaceSnapshot>('app_set_follow_shell_cwd', { tabId, enabled }),
    readRemoteFile: (tabId: string, targetPath: string, encoding?: string) =>
      invoke<string>('app_read_remote_file', { tabId, targetPath, encoding }),
    writeRemoteFile: (tabId: string, targetPath: string, content: string, encoding?: string) =>
      invoke<WorkspaceSnapshot>('app_write_remote_file', { tabId, targetPath, content, encoding }),
    createRemoteDirectory: (tabId: string, parentPath: string, name: string) =>
      invoke<WorkspaceSnapshot>('app_create_remote_directory', { tabId, parentPath, name }),
    createRemoteFile: (tabId: string, parentPath: string, name: string) =>
      invoke<WorkspaceSnapshot>('app_create_remote_file', { tabId, parentPath, name }),
    copyRemotePath: (tabId: string, targetPath: string, destinationPath: string, targetType: 'file' | 'folder') =>
      invoke<WorkspaceSnapshot>('app_copy_remote_path', { tabId, targetPath, destinationPath, targetType }),
    moveRemotePath: (tabId: string, targetPath: string, destinationPath: string) =>
      invoke<WorkspaceSnapshot>('app_move_remote_path', { tabId, targetPath, destinationPath }),
    renameRemotePath: (tabId: string, targetPath: string, newName: string) =>
      invoke<WorkspaceSnapshot>('app_rename_remote_path', { tabId, targetPath, newName }),
    deleteRemotePath: (tabId: string, targetPath: string, targetType: 'file' | 'folder', isSymlink = false) =>
      invoke<WorkspaceSnapshot>('app_delete_remote_path', {
        tabId,
        targetPath,
        targetType,
        targetIsSymlink: isSymlink
      }),
    changeRemotePermissions: (tabId: string, targetPath: string, options: PermissionChangeOptions) =>
      invoke<WorkspaceSnapshot>('app_change_remote_permissions', { tabId, targetPath, options }),
    resolveSshInteraction: (requestId: string, response: SshInteractionResponse) =>
      invoke<void>('app_resolve_ssh_interaction', { requestId, response }),
    resolveSudoPasswordPrompt: (requestId: string, cancelled: boolean, value?: string, save?: boolean) =>
      invoke<void>('app_resolve_sudo_password_prompt', {
        requestId,
        cancelled,
        value: cancelled ? null : (value ?? null),
        save: cancelled ? false : (save ?? false)
      }),
    setSudoPasswordPromptRendererReady: (registrationId: string, ready: boolean) =>
      invoke<void>('app_set_sudo_password_renderer_ready', { registrationId, ready }),
    resolveBackupPassword: (requestId: string, cancelled: boolean, value?: string) =>
      invoke<void>('app_resolve_backup_password', {
        requestId,
        cancelled,
        value: cancelled ? null : (value ?? null)
      }),
    setBackupPasswordRendererReady: (registrationId: string, ready: boolean) =>
      invoke<void>('app_set_backup_password_renderer_ready', { registrationId, ready }),
    resolveMcpApproval: (requestId: string, approved: boolean) =>
      invoke<void>('app_resolve_mcp_approval', { requestId, approved }),
    resolveActionApproval: (requestId: string, approved: boolean) =>
      invoke<void>('app_resolve_action_approval', { requestId, approved }),
    executeAiTerminalHandoff: (requestId: string, tabId: string, command: string) =>
      invoke<void>('app_execute_ai_terminal_handoff', { requestId, tabId, command }),
    setRemoteFileAccessMode: (tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions) =>
      invoke<WorkspaceSnapshot>('app_set_remote_file_access_mode', { tabId, mode, options }),
    listSshTunnels: (tabId: string) => invoke<SshTunnelSnapshot[]>('app_list_ssh_tunnels', { tabId }),
    createSshTunnel: (tabId: string, rule: SshForwardRule) =>
      invoke<SshTunnelSnapshot[]>('app_create_ssh_tunnel', { tabId, rule }),
    startSshTunnel: (tabId: string, ruleId: string) =>
      invoke<SshTunnelSnapshot[]>('app_start_ssh_tunnel', { tabId, ruleId }),
    stopSshTunnel: (tabId: string, ruleId: string) =>
      invoke<SshTunnelSnapshot[]>('app_stop_ssh_tunnel', { tabId, ruleId }),
    deleteSshTunnel: (tabId: string, ruleId: string) =>
      invoke<SshTunnelSnapshot[]>('app_delete_ssh_tunnel', { tabId, ruleId }),
    onUiPreferencesChanged: (listener: (preferences: UiPreferences) => void) =>
      subscribe('app:ui-preferences-changed', listener),
    onSecuritySettingsChanged: subscribeSecuritySettings,
    onTerminalState: (listener: (payload: TerminalStatePayload) => void) => subscribe('terminal:state', listener),
    onTransferUpdate: (listener: (transfer: TransferTask) => void) => subscribe('transfer:update', listener),
    onSerialTransferProgress: (listener: (progress: SerialTransferProgress) => void) =>
      subscribe('serial:transfer-progress', listener),
    onWorkspaceSnapshot: (listener: (snapshot: WorkspaceSnapshot) => void) =>
      subscribeReady('workspace:snapshot', listener),
    onSessionMetrics: (listener: (payload: SessionMetricsUpdate) => void) =>
      subscribe('workspace:sessionMetrics', listener),
    onRemoteFilesChanged: (listener: (payload: RemoteFilesUpdate) => void) =>
      subscribe('workspace:remote-files', listener),
    onSshInteraction: (listener: (request: SshInteractionRequest) => void) =>
      subscribeReady('ssh:interaction', listener),
    onSudoPasswordPrompt: (listener: (request: SudoPasswordRequest) => void) =>
      subscribeReady('sudo:password-request', listener),
    onSudoPasswordPromptCancelled: (listener: (payload: { requestId: string }) => void) =>
      subscribe('sudo:password-request-cancelled', listener),
    onBackupPasswordRequest: (listener: (request: BackupPasswordRequest) => void) =>
      subscribeReady('backup:password-request', listener),
    onActionApprovalRequest: (listener: (request: ActionApprovalRequest) => void) =>
      subscribe('action:approval-request', listener),
    onMcpApprovalRequest: (listener: (request: McpApprovalRequest) => void) =>
      subscribe('action:approval-request', listener)
  } satisfies FileTermDesktopApi
}
