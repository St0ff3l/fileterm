import { app, BrowserWindow, dialog, ipcMain, type WebContents } from 'electron'
import { readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { randomUUID } from 'node:crypto'
import path from 'node:path'
import type {
  CommandSendPreferences,
  CommandExecutionOptions,
  CommandFolder,
  TerminalCommandHistoryEntry,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionImportOptions,
  ConnectionImportPreviewItem,
  CreateProfileInput,
  SshForwardRule
} from '@fileterm/core'
import type { IpcServices, IpcWindowOptions } from './types.js'
import { exportProfiles, previewExternalConnectionJson, previewSshConfig } from '../services/connection-config-codec.js'
import { WebDavSyncService } from '../services/webdav-sync-service.js'

export function registerWorkspaceHandlers(services: IpcServices, options: IpcWindowOptions) {
  const { workspaceService, broadcastSnapshot } = services
  const webDavSync = new WebDavSyncService(
    app.getPath('userData'),
    async () => (await workspaceService.getConnectionLibrary()).profiles,
    async (items) => workspaceService.importProfiles(items)
  )
  const connectionImportPlans = new Map<string, ConnectionImportPreviewItem[]>()

  ipcMain.handle('workspace:getSnapshot', (event) => {
    if (isWorkspaceWindow(event.sender)) {
      workspaceService.bindWorkspaceSender(event.sender)
    }
    return workspaceService.getSnapshot()
  })

  ipcMain.handle('workspace:getConnectionLibrary', () => {
    return workspaceService.getConnectionLibrary()
  })

  ipcMain.handle('workspace:previewConnectionImport', async () => {
    const items = await selectConnectionImportItems()
    if (!items) return null
    const profiles = (await workspaceService.getConnectionLibrary()).profiles
    const byEndpoint = new Map(profiles.map((profile) => [connectionEndpointKey(profile), profile.id]))
    const planId = randomUUID()
    const planned = items.map((item) => ({
      ...item,
      id: randomUUID(),
      ...(item.input ? { conflictProfileId: byEndpoint.get(connectionEndpointKey(item.input)) } : {})
    }))
    connectionImportPlans.set(planId, planned)
    return {
      id: planId,
      items: planned.map(({ input, ...item }) => item)
    }
  })

  ipcMain.handle(
    'workspace:commitConnectionJsonImport',
    async (_, planId: string, importOptions: ConnectionImportOptions) => {
      const items = connectionImportPlans.get(planId)
      if (!items) throw new Error('导入预览已过期，请重新选择文件。')
      connectionImportPlans.delete(planId)
      const result = await workspaceService.importProfiles(items, importOptions)
      if (result.imported || result.overwritten) broadcastSnapshot(await workspaceService.getSnapshot())
      return result
    }
  )

  ipcMain.handle('workspace:exportConnections', async (event, format: 'fileterm' | 'compatible') => {
    const target = await dialog.showSaveDialog({
      defaultPath: `fileterm-connections.${format === 'fileterm' ? 'json' : 'connect_config.json'}`,
      filters: [{ name: 'JSON', extensions: ['json'] }]
    })
    if (target.canceled || !target.filePath) return false
    const library = await workspaceService.getConnectionLibrary()
    await writeFile(target.filePath, JSON.stringify(exportProfiles(library.profiles, format, true), null, 2), 'utf8')
    return true
  })

  ipcMain.handle('workspace:exportConnectionsAsFiles', async (_, format: 'fileterm' | 'compatible') => {
    const target = await dialog.showOpenDialog({
      properties: ['openDirectory', 'createDirectory'],
      title: '选择连接导出目录'
    })
    if (target.canceled || !target.filePaths[0]) return false
    const library = await workspaceService.getConnectionLibrary()
    const usedNames = new Set<string>()
    await Promise.all(
      library.profiles.map(async (profile) => {
        const baseName = safeExportFilename(profile.name, profile.id, usedNames)
        const exported = exportProfiles([profile], format, true)
        const payload = Array.isArray(exported) ? exported[0] : exported
        await writeFile(path.join(target.filePaths[0], `${baseName}.json`), JSON.stringify(payload, null, 2), 'utf8')
      })
    )
    return true
  })

  ipcMain.handle('workspace:listSshTunnels', async (_, tabId: string) => workspaceService.listSshTunnels(tabId))
  ipcMain.handle('workspace:createSshTunnel', async (_, tabId: string, rule: SshForwardRule) =>
    workspaceService.createSshTunnel(tabId, rule)
  )
  ipcMain.handle('workspace:startSshTunnel', async (_, tabId: string, ruleId: string) =>
    workspaceService.startSshTunnel(tabId, ruleId)
  )
  ipcMain.handle('workspace:stopSshTunnel', async (_, tabId: string, ruleId: string) =>
    workspaceService.stopSshTunnel(tabId, ruleId)
  )
  ipcMain.handle('workspace:deleteSshTunnel', async (_, tabId: string, ruleId: string) =>
    workspaceService.deleteSshTunnel(tabId, ruleId)
  )

  ipcMain.handle('workspace:getWebDavSyncConfig', async () => webDavSync.getConfig())
  ipcMain.handle('workspace:saveWebDavSyncConfig', async (_, input) => webDavSync.saveConfig(input))
  ipcMain.handle('workspace:uploadWebDavSync', async () => webDavSync.upload())
  ipcMain.handle('workspace:downloadWebDavSync', async () => {
    const result = await webDavSync.download()
    if (result.imported) broadcastSnapshot(await workspaceService.getSnapshot())
    return result
  })

  ipcMain.handle('workspace:createProfile', async (_, input: CreateProfileInput) => {
    const snapshot = await workspaceService.createProfile(input)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:updateProfile', async (_, profileId: string, input: CreateProfileInput) => {
    const snapshot = await workspaceService.updateProfile(profileId, input)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:deleteProfile', async (_, profileId: string) => {
    const snapshot = await workspaceService.deleteProfile(profileId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:createFolder', async (_, name: string, parentId?: string) => {
    const snapshot = await workspaceService.createFolder(name, parentId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:updateFolder', async (_, folderId: string, updates: Partial<ConnectionFolder>) => {
    const snapshot = await workspaceService.updateFolder(folderId, updates)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:deleteFolder', async (_, folderId: string) => {
    const snapshot = await workspaceService.deleteFolder(folderId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle(
    'workspace:updateEntityOrder',
    async (_, id: string, newParentId: string | undefined, newOrder: number) => {
      const snapshot = await workspaceService.updateEntityOrder(id, newParentId, newOrder)
      broadcastSnapshot(snapshot)
      return snapshot
    }
  )

  ipcMain.handle('workspace:createCommandFolder', async (_, name: string, parentId?: string) => {
    const snapshot = await workspaceService.createCommandFolder(name, parentId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:updateCommandFolder', async (_, folderId: string, updates: Partial<CommandFolder>) => {
    const snapshot = await workspaceService.updateCommandFolder(folderId, updates)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:deleteCommandFolder', async (_, folderId: string) => {
    const snapshot = await workspaceService.deleteCommandFolder(folderId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle(
    'workspace:updateCommandOrder',
    async (_, id: string, newParentId: string | undefined, newOrder: number) => {
      const snapshot = await workspaceService.updateCommandOrder(id, newParentId, newOrder)
      broadcastSnapshot(snapshot)
      return snapshot
    }
  )

  ipcMain.handle('workspace:createCommandTemplate', async (_, input: CommandTemplateInput) => {
    const snapshot = await workspaceService.createCommandTemplate(input)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:updateCommandTemplate', async (_, commandId: string, input: CommandTemplateInput) => {
    const snapshot = await workspaceService.updateCommandTemplate(commandId, input)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:deleteCommandTemplate', async (_, commandId: string) => {
    const snapshot = await workspaceService.deleteCommandTemplate(commandId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle(
    'workspace:executeCommandTemplate',
    async (_, tabId: string, commandId: string, args?: string[], options?: CommandExecutionOptions) => {
      return workspaceService.executeCommandTemplate(tabId, commandId, args, options)
    }
  )

  ipcMain.handle('workspace:getTerminalCommandHistory', async (_, profileId: string) => {
    return workspaceService.getTerminalCommandHistory(profileId)
  })

  ipcMain.handle(
    'workspace:setTerminalCommandHistory',
    async (_, profileId: string, entries: TerminalCommandHistoryEntry[]) => {
      await workspaceService.setTerminalCommandHistory(profileId, entries)
    }
  )

  ipcMain.handle('workspace:getCommandSendPreferences', async () => {
    return workspaceService.getCommandSendPreferences()
  })

  ipcMain.handle('workspace:setCommandSendPreferences', async (_, preferences: CommandSendPreferences) => {
    await workspaceService.setCommandSendPreferences(preferences)
  })

  ipcMain.handle('workspace:openProfile', async (event, profileId: string) => {
    const snapshot = await workspaceService.openProfile(profileId, event.sender)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:openProfileFromManager', async (event, profileId: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    const targetSender =
      senderWindow?.getParentWindow()?.webContents ?? options.getMainWindow()?.webContents ?? event.sender
    const snapshot = await workspaceService.openProfile(profileId, targetSender)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:activateTab', async (_, tabId: string) => {
    const snapshot = await workspaceService.activateTab(tabId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:reconnectTab', async (event, tabId: string) => {
    const snapshot = await workspaceService.reconnectTab(tabId, event.sender)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:disconnectTab', async (_, tabId: string) => {
    const snapshot = await workspaceService.disconnectTab(tabId)
    broadcastSnapshot(snapshot)
    return snapshot
  })

  ipcMain.handle('workspace:closeTab', async (_, tabId: string) => {
    const snapshot = await workspaceService.closeTab(tabId)
    broadcastSnapshot(snapshot)
    return snapshot
  })
}

async function listConnectionImportFiles(inputPath: string): Promise<string[]> {
  const info = await stat(inputPath)
  if (info.isFile()) return isSupportedConnectionImportFile(inputPath) ? [inputPath] : []
  if (!info.isDirectory()) return []
  const entries = await readdir(inputPath, { withFileTypes: true })
  const nested = await Promise.all(
    entries
      .filter((entry) => !entry.name.startsWith('.'))
      .map((entry) => listConnectionImportFiles(path.join(inputPath, entry.name)))
  )
  return nested.flat().slice(0, 500)
}

async function selectConnectionImportItems(): Promise<ConnectionImportPreviewItem[] | null> {
  const selected = await dialog.showOpenDialog({
    properties: ['openFile', 'openDirectory', 'multiSelections'],
    filters: [
      { name: '连接配置', extensions: ['json', 'config', 'txt'] },
      { name: '所有文件', extensions: ['*'] }
    ]
  })
  if (selected.canceled) return null
  const files = (await Promise.all(selected.filePaths.map(listConnectionImportFiles))).flat()
  return (
    await Promise.all(
      files.map(async (filePath) => {
        const sourceLabel = path.basename(filePath)
        try {
          const info = await stat(filePath)
          if (info.size > 2 * 1024 * 1024) {
            return [
              {
                name: sourceLabel,
                sourceLabel,
                type: 'ssh' as const,
                status: 'invalid' as const,
                reason: '导入文件超过 2 MB 限制'
              }
            ]
          }
          const contents = await readFile(filePath, 'utf8')
          const isJson = path.extname(filePath).toLowerCase() === '.json'
          const items = isJson
            ? previewExternalConnectionJson(contents, path.basename(filePath, path.extname(filePath)))
            : previewSshConfig(contents)
          return items.map((item) => ({ ...item, sourceLabel }))
        } catch (error) {
          return [
            {
              name: sourceLabel,
              sourceLabel,
              type: 'ssh' as const,
              status: 'invalid' as const,
              reason: error instanceof Error ? error.message : String(error)
            }
          ]
        }
      })
    )
  ).flat()
}

function isSupportedConnectionImportFile(filePath: string) {
  const extension = path.extname(filePath).toLowerCase()
  return (
    extension === '.json' || extension === '.config' || extension === '.txt' || path.basename(filePath) === 'config'
  )
}

function connectionEndpointKey(
  profile:
    | Pick<ConnectionImportPreviewItem, 'type' | 'host' | 'port' | 'username'>
    | { type: string; host: string; port: number; username: string }
) {
  return [profile.type, profile.host?.trim().toLowerCase(), profile.port, profile.username?.trim().toLowerCase()].join(
    '\u0000'
  )
}

function safeExportFilename(name: string, id: string, usedNames: Set<string>) {
  const normalized =
    name
      // eslint-disable-next-line no-control-regex -- exported filenames must be safe on Windows, macOS and Linux.
      .replace(/[\\/:*?"<>|\u0000-\u001f]/g, '-')
      .replace(/[. ]+$/g, '')
      .trim()
      .slice(0, 100) || 'connection'
  const reserved = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(normalized)
  const stem = reserved ? `connection-${normalized}` : normalized
  let candidate = stem
  let counter = 2
  while (usedNames.has(candidate.toLowerCase())) candidate = `${stem}-${counter++}`
  usedNames.add(candidate.toLowerCase())
  return `${candidate}-${id.slice(0, 8)}`
}

function isWorkspaceWindow(sender: WebContents) {
  try {
    const url = new URL(sender.getURL())
    const windowMode = url.searchParams.get('window')
    return !windowMode || windowMode === 'main'
  } catch {
    return false
  }
}
