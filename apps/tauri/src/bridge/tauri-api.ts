import {
  createTerminalOutputDelivery,
  type TerminalOutputDelivery,
  type TerminalOutputListener
} from './events/terminal-output'
import { subscribe, subscribeReady } from './events/subscriptions'
import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { getRuntimeMetadata } from './runtime-metadata'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { createSharedDesktopApi } from './shared-api'
import type { BridgeTransport } from './transport'
import type {
  AiChatRequest,
  AiStreamEvent,
  AppUpdateStatus,
  ConnectionExportFormat,
  ConnectionImportPlan,
  FileTermDesktopApi,
  ImportedFont,
  SshKeyFileSelection,
  TerminalZoomOperation,
  TunnelProfileType
} from '@fileterm/core'
import { APP_EVENT, dispatchAppEvent } from '../renderer/lib/app-events'

let latestNativeDropPaths: string[] = []
let latestNativeDropAt = 0
let dragDropRegistered = false
export const showCurrentWindow = () =>
  isTauri() ? invoke<void>('app_window_action', { action: 'show' }) : Promise.resolve()

const terminalDataListeners = new Set<TerminalOutputListener>()
let terminalDataChannel: Channel<TerminalOutputDelivery> | null = null
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

function invokeWithTauriChannel<T, E>(
  command: string,
  args: Record<string, unknown>,
  onEvent: (event: E) => void
): Promise<T> {
  const channel = new Channel<E>()
  let requestId: string | null = null
  let terminalEventReceived = false
  channel.onmessage = (event) => {
    onEvent(event)
    const typed = event as unknown as { type?: string }
    if (typed?.type === 'completed' || typed?.type === 'error') {
      terminalEventReceived = true
      if (requestId) {
        activeAiChatChannels.delete(requestId)
      }
    }
  }
  const typedChannel = channel as unknown as Channel<AiStreamEvent>
  pendingAiChatChannels.add(typedChannel)
  return invoke<T>(command, { ...args, channel })
    .then((result) => {
      const chatRequest = result as unknown as AiChatRequest
      if (chatRequest?.requestId) {
        requestId = chatRequest.requestId
        pendingAiChatChannels.delete(typedChannel)
        if (aiChatBridgeIsUnloading) {
          void invoke<void>('app_cancel_ai_chat', { requestId: chatRequest.requestId }).catch(() => undefined)
        } else if (!terminalEventReceived) {
          activeAiChatChannels.set(chatRequest.requestId, typedChannel)
        }
      } else {
        pendingAiChatChannels.delete(typedChannel)
      }
      return result
    })
    .catch((error) => {
      pendingAiChatChannels.delete(typedChannel)
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

  const channel = new Channel<TerminalOutputDelivery>()
  channel.onmessage = createTerminalOutputDelivery(
    terminalDataListeners,
    (deliveryId) => invoke<void>('app_ack_terminal_data', { channelId: channel.id, deliveryId }),
    (message) => {
      void invoke<void>('app_write_diagnostic_log', { level: 'WARN', scope: 'renderer:performance', message }).catch(
        () => undefined
      )
    }
  )
  terminalDataChannel = channel
  terminalDataRegistration = invoke<void>('app_subscribe_terminal_data', { channel })
    .then(() => {
      terminalDataRetryBackoffMs = 1000
      if (terminalDataRetryTimer !== null) {
        clearTimeout(terminalDataRetryTimer)
        terminalDataRetryTimer = null
      }
    })
    .catch((error) => {
      terminalDataChannel = null
      scheduleTerminalDataRetry(error)
    })
    .finally(() => {
      terminalDataRegistration = null
    })
}

function subscribeTerminalData(listener: TerminalOutputListener) {
  terminalDataListeners.add(listener)
  ensureTerminalDataChannel()
  return () => {
    terminalDataListeners.delete(listener)
  }
}

function registerNativeDragDrop() {
  if (dragDropRegistered) return
  dragDropRegistered = true
  void getCurrentWindow()
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
        dispatchAppEvent(APP_EVENT.tauriNativeDrop, {
          paths,
          consume: clearNativeDropFallback,
          position: {
            x: event.payload.position.x,
            y: event.payload.position.y
          }
        })
      }
    })
    .catch(() => undefined)
}

function takeNativeDropPaths(files: File[]) {
  const isFresh = Date.now() - latestNativeDropAt < 5_000
  if (isFresh && (files.length === 0 || latestNativeDropPaths.length === files.length)) {
    const paths = latestNativeDropPaths
    clearNativeDropFallback()
    return paths
  }
  return []
}

export async function createTauriApi(): Promise<FileTermDesktopApi> {
  window.addEventListener('pagehide', cancelAiChatsForPageHide, { once: true })
  registerNativeDragDrop()

  const metadata = await getRuntimeMetadata()
  const transport: BridgeTransport = {
    invoke: <T>(command: string, args?: Record<string, unknown>) =>
      args === undefined ? invoke<T>(command) : invoke<T>(command, args),
    invokeWithChannel: invokeWithTauriChannel,
    subscribe,
    subscribeReady
  }

  return createSharedDesktopApi(
    transport,
    {
      ...metadata,
      runtimeName: 'Tauri',
      isDesktop: true
    },
    {
      getUpdateStatus: () => invoke<AppUpdateStatus>('app_get_update_status'),
      checkForUpdates: () => invoke<AppUpdateStatus>('app_check_for_updates'),
      downloadUpdate: () => invoke<void>('app_download_update'),
      installUpdate: () => invoke<void>('app_install_update'),
      onUpdateStatus: (listener: (status: AppUpdateStatus) => void) => subscribe('app:update-status', listener),
      readClipboardText: () => invoke<string>('app_read_clipboard_text'),
      writeClipboardText: (text: string) => invoke<void>('app_write_clipboard_text', { text }),
      openConnectionManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'connection-manager' } }),
      openCommandManagerWindow: () => invoke<void>('app_open_window', { input: { kind: 'command-manager' } }),
      openConnectionFormWindow: (mode: 'create' | 'edit', profileId?: string) =>
        invoke<void>('app_open_window', { input: { kind: 'connection-form', mode, profileId } }),
      openCommandFormWindow: (mode: 'create' | 'edit', commandId?: string, folderId?: string, command?: string) =>
        invoke<void>('app_open_window', {
          input: { kind: 'command-form', mode, commandId, folderId, command }
        }),
      openProxyFormWindow: (mode: 'create' | 'edit', profileId?: string, proxyType?: 'socks5' | 'http') =>
        invoke<void>('app_open_window', {
          input: { kind: 'proxy-form', mode, profileId, encoding: proxyType }
        }),
      openTunnelFormWindow: (mode: 'create' | 'edit', profileId?: string, tunnelType?: TunnelProfileType) =>
        invoke<void>('app_open_window', {
          input: { kind: 'tunnel-form', mode, profileId, encoding: tunnelType }
        }),
      openFileEditorWindow: (input) =>
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
      saveSessionLog: (tabId: string) => invoke<string | null>('app_save_session_log', { tabId }),
      minimizeCurrentWindow: () => invoke<void>('app_window_action', { action: 'minimize' }),
      showCurrentWindow,
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
      onWindowMaximizedChange: (listener: (isMaximized: boolean) => void) =>
        subscribe('app:window-maximized-change', listener),
      onFileEditorCloseRequest: (listener: () => void) => subscribe('app:file-editor-close-request', listener),
      onTerminalData: subscribeTerminalData,
      selectLocalFiles: (defaultPath?: string) =>
        invoke<string[]>('app_select_local_files', { defaultPath: defaultPath ?? null }),
      selectLocalDirectory: (defaultPath?: string) =>
        invoke<string | null>('app_select_local_directory', { defaultPath: defaultPath ?? null }),
      importFont: () => invoke<ImportedFont | null>('app_import_font'),
      selectSshKeyFile: () => invoke<SshKeyFileSelection | null>('app_select_ssh_key_file'),
      previewConnectionImport: (source: 'files' | 'folder' = 'files') =>
        invoke<ConnectionImportPlan | null>('app_preview_connection_import', { source }),
      exportConnections: (format: ConnectionExportFormat) => invoke<boolean>('app_export_connections', { format }),
      exportConnectionsAsFiles: (format: ConnectionExportFormat) =>
        invoke<boolean>('app_export_connections_as_files', { format }),
      getDroppedFilePaths: (files: File[]) => takeNativeDropPaths(files),
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
          return invoke<void>('app_window_action', { action: 'quit' })
        }
        return invoke<void>('app_window_action', { action: 'hide' })
      }
    }
  )
}
