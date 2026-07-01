import { ipcMain } from 'electron'
import type { PermissionChangeOptions, RemoteFileAccessOptions } from '@fileterm/core'
import type { IpcServices } from './types.js'

export function registerRemoteFilesHandlers(services: IpcServices) {
  const { workspaceService } = services

  ipcMain.handle('remoteFiles:openPath', (_, tabId: string, targetPath: string) =>
    workspaceService.openRemotePath(tabId, targetPath)
  )

  ipcMain.handle('remoteFiles:setFollowShellCwd', (_, tabId: string, enabled: boolean) =>
    workspaceService.setFollowShellCwd(tabId, enabled)
  )

  ipcMain.handle('remoteFiles:setFileAccessMode', (_, tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions) =>
    workspaceService.setRemoteFileAccessMode(tabId, mode, options)
  )

  ipcMain.handle('remoteFiles:readFile', (_, tabId: string, targetPath: string, encoding?: string) =>
    workspaceService.readRemoteFile(tabId, targetPath, encoding)
  )

  ipcMain.handle('remoteFiles:writeFile', (_, tabId: string, targetPath: string, content: string, encoding?: string) =>
    workspaceService.writeRemoteFile(tabId, targetPath, content, encoding)
  )

  ipcMain.handle('remoteFiles:createDirectory', (_, tabId: string, parentPath: string, name: string) =>
    workspaceService.createRemoteDirectory(tabId, parentPath, name)
  )

  ipcMain.handle('remoteFiles:createFile', (_, tabId: string, parentPath: string, name: string) =>
    workspaceService.createRemoteFile(tabId, parentPath, name)
  )

  ipcMain.handle('remoteFiles:copyPath', (_, tabId: string, targetPath: string, destinationPath: string, targetType: 'file' | 'folder') =>
    workspaceService.copyRemotePath(tabId, targetPath, destinationPath, targetType)
  )

  ipcMain.handle('remoteFiles:movePath', (_, tabId: string, targetPath: string, destinationPath: string) =>
    workspaceService.moveRemotePath(tabId, targetPath, destinationPath)
  )

  ipcMain.handle('remoteFiles:renamePath', (_, tabId: string, targetPath: string, newName: string) =>
    workspaceService.renameRemotePath(tabId, targetPath, newName)
  )

  ipcMain.handle('remoteFiles:deletePath', (_, tabId: string, targetPath: string, targetType: 'file' | 'folder') =>
    workspaceService.deleteRemotePath(tabId, targetPath, targetType)
  )

  ipcMain.handle('remoteFiles:changePermissions', (_, tabId: string, targetPath: string, options: PermissionChangeOptions) =>
    workspaceService.changeRemotePermissions(tabId, targetPath, options)
  )
}
