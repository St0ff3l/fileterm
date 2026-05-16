import { ipcMain } from 'electron'
import type { CreateProfileInput } from '@termdock/core'
import { FileProfileRepository } from './services/file-profile-repository.js'
import { LocalFilesService } from './services/local-files-service.js'
import { WorkspaceService, seedProfiles } from './services/workspace-service.js'

export function registerIpcHandlers(userDataPath: string) {
  const workspaceService = new WorkspaceService(
    new FileProfileRepository(userDataPath, seedProfiles)
  )
  const localFilesService = new LocalFilesService()

  ipcMain.handle('workspace:getSnapshot', () => workspaceService.getSnapshot())
  ipcMain.handle('workspace:createProfile', (_, input: CreateProfileInput) =>
    workspaceService.createProfile(input)
  )
  ipcMain.handle('workspace:updateProfile', (_, profileId: string, input: CreateProfileInput) =>
    workspaceService.updateProfile(profileId, input)
  )
  ipcMain.handle('workspace:deleteProfile', (_, profileId: string) =>
    workspaceService.deleteProfile(profileId)
  )
  ipcMain.handle('workspace:openProfile', (event, profileId: string) =>
    workspaceService.openProfile(profileId, event.sender)
  )
  ipcMain.handle('workspace:activateTab', (_, tabId: string) =>
    workspaceService.activateTab(tabId)
  )
  ipcMain.handle('workspace:closeTab', (_, tabId: string) =>
    workspaceService.closeTab(tabId)
  )
  ipcMain.handle('localFiles:listDirectory', (_, dirPath?: string) =>
    localFilesService.listDirectory(dirPath)
  )
  ipcMain.handle('transfer:queueUpload', (_, fileNames: string[]) =>
    workspaceService.queueUpload(fileNames)
  )
  ipcMain.handle('terminal:write', (_, tabId: string, data: string) =>
    workspaceService.writeToTerminal(tabId, data)
  )
  ipcMain.handle('terminal:resize', (_, tabId: string, cols: number, rows: number) =>
    workspaceService.resizeTerminal(tabId, cols, rows)
  )
  ipcMain.handle('remoteFiles:openPath', (_, tabId: string, targetPath: string) =>
    workspaceService.openRemotePath(tabId, targetPath)
  )
}
