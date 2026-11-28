import type {
  AppUpdateStatus,
  ConnectionExportFormat,
  ConnectionImportPlan,
  FileTermDesktopApi,
  ImportedFont,
  SshKeyFileSelection,
  TransferTargetOptions,
  WorkspaceSnapshot
} from '@fileterm/core'
import { APP_EVENT, dispatchAppEvent } from '../renderer/lib/app-events'
import { createSharedDesktopApi } from './shared-api'
import { WebBridgeTransport } from './web-transport'

function triggerBrowserDownload(url: string, fileName?: string) {
  const link = document.createElement('a')
  link.href = url
  link.rel = 'noopener'
  if (fileName) {
    link.download = fileName
  } else {
    link.download = ''
  }
  link.style.display = 'none'
  document.body.appendChild(link)
  link.click()
  link.remove()
}

function pickBrowserFiles(options: { accept?: string; multiple?: boolean; directory?: boolean }): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.style.display = 'none'
    if (options.accept) input.accept = options.accept
    if (options.multiple) input.multiple = true
    if (options.directory) {
      input.webkitdirectory = true
    }

    let finished = false
    const cleanup = () => {
      window.removeEventListener('focus', handleFocusBack)
      input.remove()
    }
    const settle = (files: File[]) => {
      if (finished) return
      finished = true
      cleanup()
      resolve(files)
    }
    const handleFocusBack = () => {
      setTimeout(() => {
        if (!finished && (!input.files || input.files.length === 0)) {
          settle([])
        }
      }, 300)
    }

    input.addEventListener('change', () => {
      settle(Array.from(input.files ?? []))
    })
    input.addEventListener('cancel', () => {
      settle([])
    })
    window.addEventListener('focus', handleFocusBack)
    document.body.appendChild(input)
    input.click()
  })
}

async function uploadMultipart<T>(url: string, files: File[]): Promise<T> {
  const formData = new FormData()
  for (const file of files) {
    formData.append('file', file, file.name)
  }
  const response = await fetch(url, {
    method: 'POST',
    credentials: 'same-origin',
    body: formData
  })
  const payload = (await response.json().catch(() => null)) as (T & { error?: string }) | null
  if (!response.ok) {
    throw new Error(payload?.error || `Upload failed with HTTP ${response.status}`)
  }
  return payload as T
}

export async function createWebApi(): Promise<FileTermDesktopApi> {
  const transport = new WebBridgeTransport()
  const metadata = await transport.bootstrap()

  const disabledUpdateStatus = (): AppUpdateStatus => ({
    state: 'unsupported',
    currentVersion: metadata.appVersion,
    updateMode: 'release-page'
  })

  const shared = createSharedDesktopApi(
    transport,
    {
      ...metadata,
      platform: 'web',
      runtimeName: 'Web',
      isDesktop: false
    },
    {
      getUpdateStatus: async () => disabledUpdateStatus(),
      checkForUpdates: async () => disabledUpdateStatus(),
      downloadUpdate: async () => undefined,
      installUpdate: async () => undefined,
      onUpdateStatus: () => () => undefined,
      readClipboardText: async () => {
        if (!navigator.clipboard?.readText) {
          throw new Error('当前浏览器环境不支持读取剪贴板')
        }
        return navigator.clipboard.readText()
      },
      writeClipboardText: async (text: string) => {
        if (navigator.clipboard?.writeText) {
          await navigator.clipboard.writeText(text)
          return
        }
        const area = document.createElement('textarea')
        area.value = text
        area.style.position = 'fixed'
        area.style.opacity = '0'
        document.body.appendChild(area)
        area.select()
        document.execCommand('copy')
        area.remove()
      },
      openConnectionManagerWindow: async () => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, { kind: 'connection-manager' })
      },
      openCommandManagerWindow: async () => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, { kind: 'command-manager' })
      },
      openConnectionFormWindow: async (mode, profileId) => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, { kind: 'connection-form', mode, profileId })
      },
      openCommandFormWindow: async (mode, commandId, folderId, command) => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, {
          kind: 'command-form',
          mode,
          commandId,
          folderId,
          command
        })
      },
      openProxyFormWindow: async (mode, profileId, proxyType) => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, {
          kind: 'proxy-form',
          mode,
          profileId,
          proxyType
        })
      },
      openTunnelFormWindow: async (mode, profileId, tunnelType) => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, {
          kind: 'tunnel-form',
          mode,
          profileId,
          tunnelType
        })
      },
      openFileEditorWindow: async (input) => {
        dispatchAppEvent(APP_EVENT.webOpenWindow, {
          kind: 'file-editor',
          ...input
        })
      },
      openExternalUrl: async (url: string) => {
        const parsed = new URL(url, window.location.href)
        if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:' && parsed.protocol !== 'mailto:') {
          throw new Error('仅支持打开 http/https/mailto 链接')
        }
        window.open(parsed.toString(), '_blank', 'noopener,noreferrer')
      },
      openLogsDirectory: async () => {
        triggerBrowserDownload('/api/web/logs/app.log', 'fileterm-app.log')
      },
      saveSessionLog: async (tabId: string) => {
        triggerBrowserDownload(`/api/web/session-log/${encodeURIComponent(tabId)}`)
        return `session-${tabId}.log`
      },
      minimizeCurrentWindow: async () => undefined,
      showCurrentWindow: async () => undefined,
      isCurrentWindowMaximized: async () => false,
      toggleMaximizeCurrentWindow: async () => undefined,
      closeCurrentWindow: async () => {
        dispatchAppEvent(APP_EVENT.webCloseWindow)
      },
      confirmCloseCurrentFileEditor: async () => {
        dispatchAppEvent(APP_EVENT.webCloseWindow)
      },
      cancelCloseCurrentFileEditor: async () => undefined,
      showWindowMenu: async () => undefined,
      reloadCurrentWindow: async () => {
        window.location.reload()
      },
      toggleDevtools: async () => undefined,
      requestCloseCurrentWindow: async () => {
        dispatchAppEvent(APP_EVENT.webCloseWindow)
      },
      requestQuitApp: async () => undefined,
      onWindowMaximizedChange: () => () => undefined,
      onFileEditorCloseRequest: () => () => undefined,
      onTerminalData: transport.subscribeTerminalData,
      selectLocalFiles: async () => {
        const files = await pickBrowserFiles({ multiple: true })
        if (!files.length) return []
        const result = await uploadMultipart<{ paths: string[] }>('/api/web/stage-upload', files)
        return result.paths
      },
      selectLocalDirectory: async () => null,
      importFont: async () => {
        const files = await pickBrowserFiles({ accept: '.ttf,.otf', multiple: false })
        if (!files.length) return null
        return uploadMultipart<ImportedFont>('/api/web/import-font', files)
      },
      selectSshKeyFile: async () => {
        const files = await pickBrowserFiles({ multiple: false })
        if (!files.length) return null
        return uploadMultipart<SshKeyFileSelection>('/api/web/stage-ssh-key', files)
      },
      previewConnectionImport: async (source: 'files' | 'folder' = 'files') => {
        const files = await pickBrowserFiles({
          accept: source === 'folder' ? undefined : '.json,.config,.txt',
          multiple: true,
          directory: source === 'folder'
        })
        if (!files.length) return null
        return uploadMultipart<ConnectionImportPlan>('/api/web/preview-connection-import', files)
      },
      exportConnections: async (format: ConnectionExportFormat) => {
        const extension = format === 'compatible' ? 'json' : 'fileterm.json'
        triggerBrowserDownload(
          `/api/web/export-connections?format=${encodeURIComponent(format)}`,
          `fileterm-connections.${extension}`
        )
        return true
      },
      exportConnectionsAsFiles: async (format: ConnectionExportFormat) => {
        const extension = format === 'compatible' ? 'json' : 'fileterm.json'
        triggerBrowserDownload(
          `/api/web/export-connections?format=${encodeURIComponent(format)}`,
          `fileterm-connections.${extension}`
        )
        return true
      },
      getDroppedFilePaths: () => [],
      onWindowCloseRequest: () => () => undefined,
      onRequestCloseActiveWorkspaceItem: () => () => undefined,
      onNewTabRequest: () => () => undefined,
      onSplitPaneRequest: () => () => undefined,
      onFocusPaneRequest: () => () => undefined,
      onTerminalZoomRequest: () => () => undefined,
      onTerminalGestureZoomRequest: () => () => undefined,
      confirmCloseWindow: async () => undefined
    }
  )

  return {
    ...shared,
    downloadRemotePath: async (
      tabId: string,
      remotePath: string,
      targetType: 'file' | 'folder',
      localDirectory: string,
      options?: TransferTargetOptions
    ) => {
      if (!localDirectory) {
        const response = await fetch('/api/web/prepare-download', {
          method: 'POST',
          credentials: 'same-origin',
          headers: {
            'Content-Type': 'application/json',
            Accept: 'application/json'
          },
          body: JSON.stringify({
            tabId,
            remotePath,
            targetType,
            options: options ?? null
          })
        })
        const payload = (await response.json().catch(() => null)) as {
          token?: string
          fileName?: string
          snapshot?: WorkspaceSnapshot
          error?: string
        } | null
        if (!response.ok || !payload?.token) {
          throw new Error(payload?.error || `准备下载失败 (HTTP ${response.status})`)
        }
        triggerBrowserDownload(`/api/web/downloads/${encodeURIComponent(payload.token)}`, payload.fileName)
        return payload.snapshot ?? (await shared.getSnapshot())
      }
      return shared.downloadRemotePath(tabId, remotePath, targetType, localDirectory, options)
    }
  }
}
