import { Channel, invoke } from '@tauri-apps/api/core'
import { getName, getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type {
  AppUpdateStatus,
  FileTermDesktopApi,
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
  SshKeyFileSelection,
  SshKeyImportResult,
  SshKeyMetadata,
  ImportSshKeyInput,
  LocalFileItem
} from '@fileterm/core'

const unsupported = (name: string, ..._args: unknown[]) =>
  Promise.reject(new Error(`Tauri command not implemented yet: ${name}`))

let latestNativeDropPaths: string[] = []
let latestNativeDropAt = 0
const terminalDataListeners = new Set<(payload: TerminalDataPayload) => void>()
let terminalDataChannel: Channel<TerminalDataPayload> | null = null
let terminalDataRegistration: Promise<void> | null = null

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
    .catch(() => {
      terminalDataChannel = null
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
void getCurrentWindow()
  .onDragDropEvent((event) => {
    if (event.payload.type === 'enter' || event.payload.type === 'over') {
      window.dispatchEvent(
        new CustomEvent('fileterm:tauri-native-drag-over', {
          detail: { position: event.payload.position }
        })
      )
      return
    }

    if (event.payload.type === 'drop') {
      const paths = [...event.payload.paths]
      latestNativeDropPaths = paths
      latestNativeDropAt = Date.now()

      // WRY's DOM drop event often exposes File objects without native paths.
      // Publish the native event directly so the renderer can upload the
      // absolute paths without depending on the browser FileList timing.
      window.dispatchEvent(
        new CustomEvent('fileterm:tauri-native-drop', {
          detail: {
            paths,
            position: {
              x: event.payload.position.x,
              y: event.payload.position.y
            }
          }
        })
      )

      // The custom event is the primary path. Clear the fallback immediately
      // so a second DOM drop callback cannot enqueue the same files twice.
      latestNativeDropPaths = []
      latestNativeDropAt = 0
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
    latestNativeDropPaths = []
    latestNativeDropAt = 0
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
  // Tauri's `listen()` is async — under React strict mode (dev double-mount)
  // the first listener's `listen()` promise may still be pending when the
  // second mount registers another listener. If the first promise resolves
  // after the cleanup, the underlying Tauri callback stays registered until
  // `unlisten` IPC completes, and in that window the same event can be
  // delivered to both the stale and the live listener — surfacing as
  // duplicated terminal echo ("clear" → "clearclear") and double newlines.
  //
  // To close that window we register the callback id synchronously via
  // `transformCallback`, so the cleanup path can revoke it immediately
  // (without waiting on IPC), then drive `plugin:event|listen` /
  // `plugin:event|unlisten` in the background.
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__?: {
        transformCallback?: (cb: (payload: unknown) => void) => number
        unregisterCallback?: (id: number) => void
      }
    }
  ).__TAURI_INTERNALS__

  let revoked = false
  let eventId: number | undefined
  let pendingUnlisten = false

  const cleanup = () => {
    revoked = true
    // Revoke the JS-side callback first so any in-flight event delivery
    // (between `listen` resolving and `unlisten` completing) becomes a
    // no-op even if the backend still has the listener registered.
    if (callbackId !== undefined && internals?.unregisterCallback) {
      internals.unregisterCallback(callbackId)
    }
    if (eventId !== undefined && !pendingUnlisten) {
      pendingUnlisten = true
      void invoke('plugin:event|unlisten', { event: eventName, eventId }).catch(() => undefined)
    }
  }

  if (!internals?.transformCallback) {
    // Fallback: no Tauri internals available yet — no-op subscription.
    return () => undefined
  }

  const callbackId = internals.transformCallback((event) => {
    if (revoked) return
    const payload = (event as { payload?: T })?.payload
    if (payload !== undefined) listener(payload)
  })

  void invoke<number>('plugin:event|listen', {
    event: eventName,
    target: { kind: 'Any' },
    handler: callbackId
  })
    .then((id) => {
      if (revoked) {
        // Already cleaned up — tell the backend to drop the listener.
        eventId = id
        pendingUnlisten = true
        void invoke('plugin:event|unlisten', { event: eventName, eventId: id }).catch(() => undefined)
      } else {
        eventId = id
      }
    })
    .catch(() => {
      // If listen failed, revoke the callback so it doesn't leak.
      if (callbackId !== undefined && internals.unregisterCallback) {
        internals.unregisterCallback(callbackId)
      }
    })

  return cleanup
}

export function createTauriApi(): FileTermDesktopApi {
  const userAgent = navigator.userAgent.toLowerCase()
  const platform = userAgent.includes('mac') ? 'darwin' : userAgent.includes('win') ? 'win32' : 'linux'
  const arch = 'unknown'
  const api = {
    platform,
    arch,
    appVersion: '0.0.0',
    appName: 'FileTerm',
    isDesktop: true,
    getUpdateStatus: () => invoke<AppUpdateStatus>('app_get_update_status'),
    checkForUpdates: () => invoke<AppUpdateStatus>('app_check_for_updates'),
    downloadUpdate: () => invoke<void>('app_download_update'),
    installUpdate: () => invoke<void>('app_install_update'),
    onUpdateStatus: (listener: (status: AppUpdateStatus) => void) => subscribe('app:update-status', listener),
    readClipboardText: () => invoke<string>('app_read_clipboard_text'),
    writeClipboardText: (text: string) => invoke<void>('app_write_clipboard_text', { text }),
    getUiPreferences: () =>
      invoke<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>('app_get_ui_preferences'),
    setUiPreferences: (input: { theme?: 'default-dark' | 'default-light'; locale?: 'zhCN' | 'enUS' }) =>
      invoke<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>('app_set_ui_preferences', { input }),
    getUiStateItem: (key: string) => invoke<string | null>('app_get_ui_state_item', { key }),
    setUiStateItem: (key: string, value: string) => invoke<void>('app_set_ui_state_item', { key, value }),
    removeUiStateItem: (key: string) => invoke<void>('app_remove_ui_state_item', { key }),
    openConnectionManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'connection-manager' } }),
    openCommandManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'command-manager' } }),
    openConnectionFormWindow: (mode: 'create' | 'edit', profileId?: string) =>
      invoke<void>('app_open_window', { input: { kind: 'connection-form', mode, profile_id: profileId } }),
    openCommandFormWindow: (mode: 'create' | 'edit', commandId?: string, folderId?: string) =>
      invoke<void>('app_open_window', {
        input: { kind: 'command-form', mode, command_id: commandId, folder_id: folderId }
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
          tab_id: input.tabId,
          encoding: input.encoding
        }
      }),
    openExternalUrl: (url: string) => invoke<void>('app_open_external_url', { url }),
    openLogsDirectory: () => invoke<void>('app_open_logs_directory'),
    minimizeCurrentWindow: () => invoke<void>('app_window_action', { action: 'minimize' }),
    isCurrentWindowMaximized: () => invoke<boolean>('app_is_window_maximized'),
    toggleMaximizeCurrentWindow: () => invoke<void>('app_window_action', { action: 'toggle-maximize' }),
    closeCurrentWindow: () => invoke<void>('app_window_action', { action: 'close' }),
    confirmCloseCurrentFileEditor: () => invoke<void>('app_window_action', { action: 'close' }),
    cancelCloseCurrentFileEditor: () => invoke<void>('app_cancel_file_editor_close'),
    showWindowMenu: (menuType: 'app' | 'file' | 'view' | 'window', x: number, y: number) =>
      invoke<void>('app_show_window_menu', { menuType, x, y }),
    requestQuitApp: () => invoke<void>('app_window_action', { action: 'quit' }),
    listLocalDirectory: (dirPath?: string) =>
      invoke<{ path: string; items: LocalFileItem[] }>('app_list_local_directory', {
        dirPath: dirPath ?? null
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
    listSshKeys: () => invoke<SshKeyMetadata[]>('app_list_ssh_keys'),
    selectSshKeyFile: () => invoke<SshKeyFileSelection | null>('app_select_ssh_key_file'),
    importSshKey: (input?: ImportSshKeyInput) => invoke<SshKeyImportResult | null>('app_import_ssh_key', { input }),
    updateSshKeyNote: (keyId: string, note: string) =>
      invoke<SshKeyMetadata>('app_update_ssh_key_note', { keyId, note }),
    deleteSshKey: (keyId: string) => invoke<void>('app_delete_ssh_key', { keyId }),
    previewConnectionImport: () => invoke<ConnectionImportPlan | null>('app_preview_connection_import'),
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
    uploadWebDavSync: () => invoke<WebDavSyncResult>('app_upload_webdav_sync'),
    downloadWebDavSync: () => invoke<WebDavSyncResult>('app_download_webdav_sync'),
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
    ) => invoke('app_execute_command_template', { tabId, commandId, args, options: options ?? null }),
    openProfile: (profileId: string) => invoke<WorkspaceSnapshot>('app_open_profile', { profileId }),
    openProfileFromManager: (profileId: string) => invoke<WorkspaceSnapshot>('app_open_profile', { profileId }),
    activateTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_activate_tab', { tabId }),
    reconnectTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_reconnect_tab', { tabId }),
    disconnectTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_disconnect_tab', { tabId }),
    closeTab: (tabId: string) => invoke<WorkspaceSnapshot>('app_close_tab', { tabId }),

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
    setRemoteFileAccessMode: (tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions) =>
      invoke<WorkspaceSnapshot>('app_set_remote_file_access_mode', { tabId, mode, options }),
    listSshTunnels: (tabId: string) => invoke('app_list_ssh_tunnels', { tabId }),
    createSshTunnel: (tabId: string, rule: unknown) => invoke('app_create_ssh_tunnel', { tabId, rule }),
    startSshTunnel: (tabId: string, ruleId: string) => invoke('app_start_ssh_tunnel', { tabId, ruleId }),
    stopSshTunnel: (tabId: string, ruleId: string) => invoke('app_stop_ssh_tunnel', { tabId, ruleId }),
    deleteSshTunnel: (tabId: string, ruleId: string) => invoke('app_delete_ssh_tunnel', { tabId, ruleId }),

    getDroppedFilePaths: (files: File[]) => takeNativeDropPaths(files),
    onUiPreferencesChanged: (
      listener: (preferences: { theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }) => void
    ) => subscribe('app:ui-preferences-changed', listener),
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
    onSshKeysChanged: (listener: (keys: SshKeyMetadata[]) => void) => subscribe('sshKeys:changed', listener),
    onWindowCloseRequest: (listener: (event: { isQuit: boolean }) => void) =>
      subscribe('app:window-close-request', listener),
    onRequestCloseActiveWorkspaceItem: (listener: () => void) =>
      subscribe('app:close-active-workspace-item-request', listener),
    confirmCloseWindow: (action: 'quit' | 'hide' | 'cancel') => {
      if (action === 'cancel') return Promise.resolve()
      if (action === 'quit') {
        // 'quit' calls app.exit(0) on the Rust side, bypassing the
        // CloseRequested guard so Cmd+Q / tray-quit actually terminates
        // the app instead of looping back through the confirmation dialog.
        return invoke<void>('app_window_action', { action: 'quit' })
      }
      return invoke<void>('app_window_action', { action: 'minimize' })
    }
  } as unknown as FileTermDesktopApi

  // The core API exposes these as synchronous fields for Electron parity.
  // Tauri resolves app metadata asynchronously, so start with safe values and
  // hydrate them from the Rust runtime as soon as IPC is available.
  void Promise.all([invoke<string>('app_get_platform'), invoke<string>('app_get_arch'), getVersion(), getName()])
    .then(([nativePlatform, nativeArch, appVersion, appName]) => {
      Object.assign(api, {
        platform: normalizePlatform(nativePlatform),
        arch: nativeArch,
        appVersion,
        appName
      })
    })
    .catch(() => undefined)

  return new Proxy(api as FileTermDesktopApi, {
    get(target, property, receiver) {
      if (property in target) return Reflect.get(target, property, receiver)
      if (typeof property !== 'string') return undefined
      return (...args: unknown[]) => unsupported(property, ...args)
    }
  })
}
