import { Channel, invoke, transformCallback } from '@tauri-apps/api/core'
import { getName, getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type {
  AppUpdateStatus,
  BackupDownloadMode,
  BackupUploadMode,
  FileTermDesktopApi,
  S3BackupConfig,
  S3BackupConfigInput,
  S3BackupResult,
  WebDavSyncConfig,
  WebDavSyncResult,
  ConnectionImportPlan,
  ConnectionImportOptions,
  ConnectionImportResult,
  ConnectionExportFormat,
  WorkspaceSnapshot,
  PermissionChangeOptions,
  SshInteractionResponse,
  RemoteFileAccessOptions,
  TerminalDataPayload,
  TerminalStatePayload,
  TerminalCommandHistoryEntry,
  CommandSendPreferences,
  TransferTask,
  SessionMetricsUpdate,
  SshInteractionRequest,
  RemoteExecCredentials,
  SudoPasswordRequest,
  BackupPasswordRequest,
  SshKeyFileSelection,
  SshKeyImportResult,
  SshKeyMetadata,
  ImportSshKeyInput,
  LocalFileItem,
  LocalNetworkShareConnectionResult,
  SshForwardRule,
  SshTunnelSnapshot,
  CommandExecutionResult,
  TerminalZoomOperation,
  ActionApprovalRequest,
  McpApprovalRequest,
  LocalTerminalLaunchOptions,
  AiProviderSummary,
  AiProviderTestResult,
  AiChatRequest,
  AiConversation,
  AiConversationSummary,
  AiContextPreview,
  AiCopilotModeState,
  AiStreamEvent,
  CreateAiConversationInput,
  CreateAiContextPreviewInput,
  ImportedFont,
  RenameAiConversationInput,
  RetryAiChatInput,
  SaveAiProviderInput,
  SetAiContextAttachInput,
  SetAiCopilotModeInput,
  SetAiDangerousCommandRestrictionsInput,
  StartAiChatInput,
  SummarizeAiConversationTitleInput,
  TestAiProviderInput,
  McpAgentSetup,
  UiPreferences,
  UiPreferencesInput
} from '@fileterm/core'
import { APP_EVENT, dispatchAppEvent } from '../renderer/lib/app-events'

let latestNativeDropPaths: string[] = []
let latestNativeDropAt = 0
const currentWindow = getCurrentWindow()
const terminalDataListeners = new Set<(payload: TerminalDataPayload) => void>()
let terminalDataChannel: Channel<TerminalDataPayload> | null = null
let terminalDataRegistration: Promise<void> | null = null
let terminalDataRetryTimer: ReturnType<typeof setTimeout> | null = null
let terminalDataRetryBackoffMs = 1000
const TERMINAL_DATA_RETRY_MAX_BACKOFF_MS = 30_000
const pendingAiChatChannels = new Set<Channel<AiStreamEvent>>()
const activeAiChatChannels = new Map<string, Channel<AiStreamEvent>>()
let aiChatBridgeIsUnloading = false

function cancelAiChatsForPageHide() {
  aiChatBridgeIsUnloading = true
  const requestIds = [...activeAiChatChannels.keys()]
  activeAiChatChannels.clear()
  pendingAiChatChannels.clear()
  for (const requestId of requestIds) {
    // Best effort is intentional: a closing WebView cannot wait for IPC, but
    // the backend still receives a cancellation whenever it remains alive.
    void invoke<void>('app_cancel_ai_chat', { requestId }).catch(() => undefined)
  }
}

window.addEventListener('pagehide', cancelAiChatsForPageHide, { once: true })

function invokeAiChat(
  command: 'app_start_ai_chat' | 'app_retry_ai_chat',
  input: StartAiChatInput | RetryAiChatInput,
  onEvent: (event: AiStreamEvent) => void
) {
  const channel = new Channel<AiStreamEvent>()
  let requestId: string | null = null
  let terminalEventReceived = false
  channel.onmessage = (event) => {
    onEvent(event)
    if (event.type === 'completed' || event.type === 'error') {
      terminalEventReceived = true
      if (requestId) {
        activeAiChatChannels.delete(requestId)
      }
    }
  }
  pendingAiChatChannels.add(channel)
  return invoke<AiChatRequest>(command, { input, channel })
    .then((request) => {
      requestId = request.requestId
      pendingAiChatChannels.delete(channel)
      if (aiChatBridgeIsUnloading) {
        void invoke<void>('app_cancel_ai_chat', { requestId: request.requestId }).catch(() => undefined)
      } else if (!terminalEventReceived) {
        activeAiChatChannels.set(request.requestId, channel)
      }
      return request
    })
    .catch((error) => {
      pendingAiChatChannels.delete(channel)
      throw error
    })
}

function clearNativeDropFallback() {
  latestNativeDropPaths = []
  latestNativeDropAt = 0
}

function scheduleTerminalDataRetry(reason: unknown) {
  if (terminalDataRetryTimer !== null) {
    clearTimeout(terminalDataRetryTimer)
  }
  const delay = terminalDataRetryBackoffMs
  terminalDataRetryBackoffMs = Math.min(terminalDataRetryBackoffMs * 2, TERMINAL_DATA_RETRY_MAX_BACKOFF_MS)
  // Without a retry the terminal stays blank until the window is reloaded,
  // which looks like a hang. Log the failure so it's observable in the
  // devtools console, then retry with exponential backoff.
  console.warn(`[FileTerm] 终端数据订阅失败，${delay}ms 后重试`, reason)
  terminalDataRetryTimer = setTimeout(() => {
    terminalDataRetryTimer = null
    ensureTerminalDataChannel()
  }, delay)
}

function ensureTerminalDataChannel() {
  if (terminalDataChannel || terminalDataRegistration) {
    return
  }

  const channel = new Channel<TerminalDataPayload>()
  channel.onmessage = (payload) => {
    for (const listener of terminalDataListeners) {
      listener(payload)
    }
  }
  terminalDataChannel = channel
  terminalDataRegistration = invoke<void>('app_subscribe_terminal_data', { channel })
    .then(() => {
      // Subscription succeeded — reset the backoff so a future transient
      // failure starts fresh instead of inheriting the grown delay.
      terminalDataRetryBackoffMs = 1000
      if (terminalDataRetryTimer !== null) {
        clearTimeout(terminalDataRetryTimer)
        terminalDataRetryTimer = null
      }
    })
    .catch((error) => {
      // Drop the half-built channel so the next attempt creates a fresh one.
      // Then retry with exponential backoff so a transient Rust-side failure
      // doesn't permanently blank the terminal until the user reloads.
      terminalDataChannel = null
      scheduleTerminalDataRetry(error)
    })
    .finally(() => {
      terminalDataRegistration = null
    })
}

function subscribeTerminalData(listener: (payload: TerminalDataPayload) => void) {
  terminalDataListeners.add(listener)
  ensureTerminalDataChannel()
  return () => {
    terminalDataListeners.delete(listener)
  }
}

// Browser File objects in a Tauri webview intentionally do not expose their
// native filesystem path. Keep the path list from Tauri's drag-drop event so
// the existing DOM drop handler can hand main-process code real local paths.
// The list is single-use to prevent a stale native drop from being paired with
// a later browser-only drop of the same number of files.
void currentWindow
  .onDragDropEvent((event) => {
    if (event.payload.type === 'enter' || event.payload.type === 'over') {
      dispatchAppEvent(APP_EVENT.tauriNativeDragOver, {
        paths: event.payload.type === 'enter' ? [...event.payload.paths] : [],
        position: event.payload.position
      })
      return
    }

    if (event.payload.type === 'drop') {
      const paths = [...event.payload.paths]
      latestNativeDropPaths = paths
      latestNativeDropAt = Date.now()

      // WRY's DOM drop event often exposes File objects without native paths.
      // Publish the native event directly so the renderer can upload the
      // absolute paths without depending on the browser FileList timing.
      dispatchAppEvent(APP_EVENT.tauriNativeDrop, {
        paths,
        consume: clearNativeDropFallback,
        position: {
          x: event.payload.position.x,
          y: event.payload.position.y
        }
      })

      // Keep the fallback until the renderer confirms that the native drop
      // hit its remote-pane target. If coordinate probing or listener setup
      // rejects this custom event, the following DOM drop can still consume
      // the same absolute paths instead of silently doing nothing.
    }
  })
  .catch(() => undefined)

function takeNativeDropPaths(files: File[]) {
  const isFresh = Date.now() - latestNativeDropAt < 5_000
  // WRY may expose an empty DOM FileList for an OS-level drop even though the
  // native Tauri event contains every absolute path. Accept that case too;
  // requiring equal counts made external macOS Finder drops look like no-op.
  if (isFresh && (files.length === 0 || latestNativeDropPaths.length === files.length)) {
    const paths = latestNativeDropPaths
    clearNativeDropFallback()
    return paths
  }
  // 拿不到原生路径时返回空数组，而不是返回 file.name（仅文件名非完整路径，
  // 会导致后端 tokio::fs::metadata 失败，用户看到"拖拽什么都没发生"）。
  // 上层 extractDroppedLocalPaths 会 filter 掉空值，handleRemotePaneDrop
  // 拿到空数组后不会有任何动作，比用无效路径尝试上传更清晰。
  return []
}

function normalizePlatform(value: string) {
  if (value === 'macos' || value === 'darwin') return 'darwin'
  if (value === 'windows' || value === 'win32') return 'win32'
  return 'linux'
}

function subscribe<T>(eventName: string, listener: (payload: T) => void) {
  // React strict mode can clean up the first mount before Tauri's asynchronous
  // listen registration resolves. Keep the callback inert immediately, ask
  // the backend to remove the event id, and only then release the JS callback.
  // Tauri's public unlisten helper currently performs those last two steps in
  // the opposite order, leaving a race where an in-flight event tries to call
  // an id that no longer exists during Windows hot reloads and child-window
  // teardown.
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: { unregisterCallback?: (id: number) => void }
    __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener?: (event: string, eventId: number) => void }
  }
  let active = true
  let eventId: number | null = null
  let unlistenStarted = false

  const callbackId = transformCallback((event: unknown) => {
    if (!active) return
    const payload = (event as { payload?: T })?.payload
    if (payload !== undefined) listener(payload)
  })

  const unregisterFrontend = (id: number) => {
    internals.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener?.(eventName, id)
    internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
  }

  const stopListening = () => {
    if (eventId === null || unlistenStarted) return
    unlistenStarted = true
    const registeredEventId = eventId
    void invoke<void>('plugin:event|unlisten', { event: eventName, eventId: registeredEventId })
      .then(() => unregisterFrontend(registeredEventId))
      .catch(() => {
        // Keep the inert callback registered if the backend did not confirm
        // removal. This is preferable to an event targeting a missing id.
      })
  }

  void invoke<number>('plugin:event|listen', {
    event: eventName,
    target: { kind: 'Any' },
    handler: callbackId
  })
    .then((id) => {
      eventId = id
      if (!active) stopListening()
    })
    .catch(() => internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId))

  return () => {
    active = false
    stopListening()
  }
}

/**
 * A subscription whose promise resolves only once Tauri has registered the
 * native event listener. Secure remote-exec prompts use this so the backend
 * never starts a task that can only wait for an unobservable renderer event.
 */
function subscribeReady<T>(eventName: string, listener: (payload: T) => void): Promise<() => void> {
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: { unregisterCallback?: (id: number) => void }
    __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener?: (event: string, eventId: number) => void }
  }
  let active = true
  let eventId: number | null = null
  let unlistenStarted = false
  const callbackId = transformCallback((event: unknown) => {
    if (!active) return
    const payload = (event as { payload?: T })?.payload
    if (payload !== undefined) listener(payload)
  })
  const stopListening = () => {
    active = false
    if (eventId === null || unlistenStarted) return
    unlistenStarted = true
    const registeredEventId = eventId
    void invoke<void>('plugin:event|unlisten', { event: eventName, eventId: registeredEventId })
      .then(() => {
        internals.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener?.(eventName, registeredEventId)
        internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
      })
      .catch(() => undefined)
  }

  return invoke<number>('plugin:event|listen', {
    event: eventName,
    target: { kind: 'Any' },
    handler: callbackId
  })
    .then((id) => {
      eventId = id
      return stopListening
    })
    .catch((error) => {
      active = false
      internals.__TAURI_INTERNALS__?.unregisterCallback?.(callbackId)
      throw error
    })
}

export async function createTauriApi(): Promise<FileTermDesktopApi> {
  const [nativePlatform, arch, runtimeVersion, appVersion, appName] = await Promise.all([
    invoke<string>('app_get_platform'),
    invoke<string>('app_get_arch'),
    invoke<string>('app_get_runtime_version'),
    getVersion(),
    getName()
  ])
  const api = {
    platform: normalizePlatform(nativePlatform),
    arch,
    appVersion,
    appName,
    runtimeName: 'Tauri',
    runtimeVersion,
    isDesktop: true,
    getUpdateStatus: () => invoke<AppUpdateStatus>('app_get_update_status'),
    checkForUpdates: () => invoke<AppUpdateStatus>('app_check_for_updates'),
    downloadUpdate: () => invoke<void>('app_download_update'),
    installUpdate: () => invoke<void>('app_install_update'),
    onUpdateStatus: (listener: (status: AppUpdateStatus) => void) => subscribe('app:update-status', listener),
    readClipboardText: () => invoke<string>('app_read_clipboard_text'),
    writeClipboardText: (text: string) => invoke<void>('app_write_clipboard_text', { text }),
    getUiPreferences: () => invoke<UiPreferences>('app_get_ui_preferences'),
    setUiPreferences: (input: UiPreferencesInput) => invoke<UiPreferences>('app_set_ui_preferences', { input }),
    getMcpAgentSetup: () => invoke<McpAgentSetup>('app_get_mcp_agent_setup'),
    listAiProviders: () => invoke<AiProviderSummary[]>('app_list_ai_providers'),
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
      invokeAiChat('app_start_ai_chat', input, onEvent),
    retryAiChat: (input: RetryAiChatInput, onEvent: (event: AiStreamEvent) => void) =>
      invokeAiChat('app_retry_ai_chat', input, onEvent),
    cancelAiChat: (requestId: string) => invoke<void>('app_cancel_ai_chat', { requestId }),
    getUiStateItem: (key: string) => invoke<string | null>('app_get_ui_state_item', { key }),
    setUiStateItem: (key: string, value: string) => invoke<void>('app_set_ui_state_item', { key, value }),
    removeUiStateItem: (key: string) => invoke<void>('app_remove_ui_state_item', { key }),
    openConnectionManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'connection-manager' } }),
    openCommandManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'command-manager' } }),
    openConnectionFormWindow: (mode: 'create' | 'edit', profileId?: string) =>
      // `OpenWindowInput` explicitly uses camelCase serde names. Sending the
      // Rust field spelling here silently drops the id, so an edit child
      // window receives no `profileId` in its URL and falls back to an empty
      // form.
      invoke<void>('app_open_window', { input: { kind: 'connection-form', mode, profileId } }),
    openCommandFormWindow: (mode: 'create' | 'edit', commandId?: string, folderId?: string, command?: string) =>
      invoke<void>('app_open_window', {
        input: { kind: 'command-form', mode, commandId, folderId, command }
      }),
    openFileEditorWindow: (input: {
      source: 'local' | 'remote'
      path: string
      name: string
      tabId?: string
      encoding?: string
    }) =>
      invoke<void>('app_open_window', {
        input: {
          kind: 'file-editor',
          source: input.source,
          path: input.path,
          name: input.name,
          tabId: input.tabId,
          encoding: input.encoding
        }
      }),
    openExternalUrl: (url: string) => invoke<void>('app_open_external_url', { url }),
    openLogsDirectory: () => invoke<void>('app_open_logs_directory'),
    minimizeCurrentWindow: () => invoke<void>('app_window_action', { action: 'minimize' }),
    showCurrentWindow: () => invoke<void>('app_window_action', { action: 'show' }),
    isCurrentWindowMaximized: () => invoke<boolean>('app_is_window_maximized'),
    toggleMaximizeCurrentWindow: () => invoke<void>('app_window_action', { action: 'toggle-maximize' }),
    closeCurrentWindow: () => invoke<void>('app_window_action', { action: 'close' }),
    confirmCloseCurrentFileEditor: () => invoke<void>('app_window_action', { action: 'close' }),
    cancelCloseCurrentFileEditor: () => invoke<void>('app_cancel_file_editor_close'),
    showWindowMenu: (menuType: 'app' | 'file' | 'view' | 'window', x: number, y: number) =>
      invoke<void>('app_show_window_menu', { menuType, x, y }),
    reloadCurrentWindow: () => invoke<void>('app_window_action', { action: 'reload' }),
    toggleDevtools: () => invoke<void>('app_window_action', { action: 'toggle-devtools' }),
    requestCloseCurrentWindow: () => invoke<void>('app_window_action', { action: 'request-close-window' }),
    requestQuitApp: () => invoke<void>('app_window_action', { action: 'request-quit' }),
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
    selectLocalFiles: (defaultPath?: string) =>
      invoke<string[]>('app_select_local_files', { defaultPath: defaultPath ?? null }),
    selectLocalDirectory: (defaultPath?: string) =>
      invoke<string | null>('app_select_local_directory', { defaultPath: defaultPath ?? null }),
    queueUpload: (fileNames: string[]) => invoke<WorkspaceSnapshot>('app_queue_upload', { fileNames }),
    cancelTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_cancel_transfer', { transferId }),
    pauseTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_pause_transfer', { transferId }),
    resumeTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_resume_transfer', { transferId }),
    discardTransfer: (transferId: string) => invoke<WorkspaceSnapshot>('app_discard_transfer', { transferId }),
    clearTransfers: (transferIds: string[]) => invoke<WorkspaceSnapshot>('app_clear_transfers', { transferIds }),
    getTerminalCommandHistory: (profileId: string) =>
      invoke<TerminalCommandHistoryEntry[]>('app_get_terminal_command_history', { profileId }),
    setTerminalCommandHistory: (profileId: string, entries: TerminalCommandHistoryEntry[]) =>
      invoke<void>('app_set_terminal_command_history', { profileId, entries }),
    getCommandSendPreferences: () => invoke<CommandSendPreferences>('app_get_command_send_preferences'),
    setCommandSendPreferences: (preferences: CommandSendPreferences) =>
      invoke<void>('app_set_command_send_preferences', { preferences }),
    uploadFile: (tabId: string, localPath: string, remoteDirectory: string, options?: { targetName?: string }) =>
      invoke<WorkspaceSnapshot>('app_upload_file', { tabId, localPath, remoteDirectory, options: options ?? null }),
    downloadFile: (tabId: string, remotePath: string, localDirectory: string, options?: { targetName?: string }) =>
      invoke<WorkspaceSnapshot>('app_download_file', { tabId, remotePath, localDirectory, options: options ?? null }),
    downloadRemotePath: (
      tabId: string,
      remotePath: string,
      targetType: 'file' | 'folder',
      localDirectory: string,
      options?: { targetName?: string }
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
    importFont: () => invoke<ImportedFont | null>('app_import_font'),
    getImportedFontData: (fontId: string) => invoke<string | null>('app_get_imported_font_data', { fontId }),
    deleteImportedFont: (fontId: string) => invoke<boolean>('app_delete_imported_font', { fontId }),
    listSshKeys: () => invoke<SshKeyMetadata[]>('app_list_ssh_keys'),
    selectSshKeyFile: () => invoke<SshKeyFileSelection | null>('app_select_ssh_key_file'),
    importSshKey: (input?: ImportSshKeyInput) => invoke<SshKeyImportResult | null>('app_import_ssh_key', { input }),
    updateSshKeyNote: (keyId: string, note: string) =>
      invoke<SshKeyMetadata>('app_update_ssh_key_note', { keyId, note }),
    deleteSshKey: (keyId: string) => invoke<void>('app_delete_ssh_key', { keyId }),
    previewConnectionImport: (source: 'files' | 'folder' = 'files') =>
      invoke<ConnectionImportPlan | null>('app_preview_connection_import', { source }),
    commitConnectionJsonImport: (planId: string, options: ConnectionImportOptions) =>
      invoke<ConnectionImportResult>('app_commit_connection_json_import', { planId, options }),
    exportConnections: (format: ConnectionExportFormat) => invoke<boolean>('app_export_connections', { format }),
    exportConnectionsAsFiles: (format: ConnectionExportFormat) =>
      invoke<boolean>('app_export_connections_as_files', { format }),
    getWebDavSyncConfig: () => invoke<WebDavSyncConfig>('app_get_webdav_sync_config'),
    saveWebDavSyncConfig: (input: {
      enabled: boolean
      url: string
      username?: string
      remotePath: string
      allowInsecureTls?: boolean
      password?: string
    }) => invoke<WebDavSyncConfig>('app_set_webdav_sync_config', { input }),
    testWebDavSync: () => invoke<WebDavSyncResult>('app_test_webdav_sync'),
    uploadWebDavSync: (mode: BackupUploadMode) => invoke<WebDavSyncResult>('app_upload_webdav_sync', { mode }),
    downloadWebDavSync: (mode: BackupDownloadMode) => invoke<WebDavSyncResult>('app_download_webdav_sync', { mode }),
    getS3BackupConfig: () => invoke<S3BackupConfig>('app_get_s3_backup_config'),
    saveS3BackupConfig: (input: S3BackupConfigInput) => invoke<S3BackupConfig>('app_set_s3_backup_config', { input }),
    testS3Backup: () => invoke<S3BackupResult>('app_test_s3_backup'),
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
    updateProfile: (profileId: string, input: unknown) =>
      invoke<WorkspaceSnapshot>('app_update_profile', { profileId, input }),
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
    openProfileFromManager: (profileId: string) => invoke<WorkspaceSnapshot>('app_open_profile', { profileId }),
    activateTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_activate_tab', { tabId }),
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
    deleteRemotePath: (tabId: string, targetPath: string, targetType: 'file' | 'folder') =>
      invoke<WorkspaceSnapshot>('app_delete_remote_path', { tabId, targetPath, targetType }),
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
    resolveAiTerminalHandoff: (requestId: string) => invoke<void>('app_resolve_ai_terminal_handoff', { requestId }),
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

    getDroppedFilePaths: (files: File[]) => takeNativeDropPaths(files),
    onUiPreferencesChanged: (listener: (preferences: UiPreferences) => void) =>
      subscribe('app:ui-preferences-changed', listener),
    onWindowMaximizedChange: (listener: (isMaximized: boolean) => void) =>
      subscribe('app:window-maximized-change', listener),
    onFileEditorCloseRequest: (listener: () => void) => subscribe('app:file-editor-close-request', listener),
    onTerminalData: subscribeTerminalData,
    onTerminalState: (listener: (payload: TerminalStatePayload) => void) => subscribe('terminal:state', listener),
    onTransferUpdate: (listener: (transfer: TransferTask) => void) => subscribe('transfer:update', listener),
    onWorkspaceSnapshot: (listener: (snapshot: WorkspaceSnapshot) => void) => subscribe('workspace:snapshot', listener),
    onSessionMetrics: (listener: (payload: SessionMetricsUpdate) => void) =>
      subscribe('workspace:sessionMetrics', listener),
    onSshInteraction: (listener: (request: SshInteractionRequest) => void) => subscribe('ssh:interaction', listener),
    onSudoPasswordPrompt: (listener: (request: SudoPasswordRequest) => void) =>
      subscribeReady('sudo:password-request', listener),
    onBackupPasswordRequest: (listener: (request: BackupPasswordRequest) => void) =>
      subscribeReady('backup:password-request', listener),
    onActionApprovalRequest: (listener: (request: ActionApprovalRequest) => void) =>
      subscribe('action:approval-request', listener),
    onMcpApprovalRequest: (listener: (request: McpApprovalRequest) => void) =>
      subscribe('action:approval-request', listener),
    onSshKeysChanged: (listener: (keys: SshKeyMetadata[]) => void) => subscribe('sshKeys:changed', listener),
    onWindowCloseRequest: (listener: (event: { isQuit: boolean }) => void) =>
      subscribe('app:window-close-request', listener),
    onRequestCloseActiveWorkspaceItem: (listener: () => void) =>
      subscribe('app:close-active-workspace-item-request', listener),
    onNewTabRequest: (listener: () => void) => subscribe('app:new-tab-request', listener),
    onSplitPaneRequest: (listener: (direction: 'row' | 'column') => void) =>
      subscribe('app:split-pane-request', listener),
    onFocusPaneRequest: (listener: (direction: 'left' | 'right' | 'up' | 'down') => void) =>
      subscribe('app:focus-pane-request', listener),
    onTerminalZoomRequest: (listener: (operation: TerminalZoomOperation) => void) =>
      subscribe('app:terminal-zoom-request', listener),
    onTerminalGestureZoomRequest: (listener: (operation: TerminalZoomOperation) => void) =>
      subscribe('app:terminal-gesture-zoom-request', listener),
    confirmCloseWindow: (action: 'quit' | 'hide' | 'cancel') => {
      if (action === 'cancel') return Promise.resolve()
      if (action === 'quit') {
        // 'quit' calls app.exit(0) on the Rust side, bypassing the
        // CloseRequested guard so Cmd+Q / tray-quit actually terminates
        // the app instead of looping back through the confirmation dialog.
        return invoke<void>('app_window_action', { action: 'quit' })
      }
      return invoke<void>('app_window_action', { action: 'hide' })
    }
  } satisfies FileTermDesktopApi

  return api
}
