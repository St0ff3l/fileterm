import { ipcMain } from 'electron'
import type { TransferTargetOptions } from '@fileterm/core'
import type { IpcServices } from './types.js'

export function registerTransferHandlers(services: IpcServices) {
  const { workspaceService } = services

  ipcMain.handle('transfer:queueUpload', (_, fileNames: string[]) => workspaceService.queueUpload(fileNames))

  ipcMain.handle('transfer:cancel', (event, transferId: string) =>
    workspaceService.cancelTransfer(transferId, event.sender)
  )

  ipcMain.handle('transfer:pause', (event, transferId: string) =>
    workspaceService.pauseTransfer(transferId, event.sender)
  )

  ipcMain.handle('transfer:resume', (event, transferId: string) =>
    workspaceService.resumeTransfer(transferId, event.sender)
  )

  ipcMain.handle('transfer:discard', (event, transferId: string) =>
    workspaceService.discardTransfer(transferId, event.sender)
  )

  ipcMain.handle('transfer:clear', (_, transferIds: string[]) => workspaceService.clearTransfers(transferIds))

  ipcMain.handle(
    'transfer:uploadFile',
    (event, tabId: string, localPath: string, remoteDirectory: string, options?: TransferTargetOptions) =>
      workspaceService.uploadFile(tabId, localPath, remoteDirectory, event.sender, options)
  )

  ipcMain.handle(
    'transfer:downloadFile',
    (event, tabId: string, remotePath: string, localDirectory: string, options?: TransferTargetOptions) =>
      workspaceService.downloadFile(tabId, remotePath, localDirectory, event.sender, options)
  )

  ipcMain.handle(
    'transfer:downloadRemotePath',
    (
      event,
      tabId: string,
      remotePath: string,
      targetType: 'file' | 'folder',
      localDirectory: string,
      options?: TransferTargetOptions
    ) => workspaceService.downloadRemotePath(tabId, remotePath, targetType, localDirectory, event.sender, options)
  )
}
