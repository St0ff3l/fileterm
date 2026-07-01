import { app, BrowserWindow, dialog, ipcMain } from 'electron'
import type { OpenDialogOptions } from 'electron'
import type { PermissionChangeOptions } from '@fileterm/core'
import type { IpcServices } from './types.js'

export function registerLocalFilesHandlers(services: IpcServices) {
  const { localFilesService } = services

  ipcMain.handle('localFiles:listDirectory', (_, dirPath?: string) =>
    localFilesService.listDirectory(dirPath)
  )

  ipcMain.handle('localFiles:readFile', (_, filePath: string, encoding?: string) =>
    localFilesService.readFile(filePath, encoding)
  )

  ipcMain.handle('localFiles:writeFile', (_, filePath: string, content: string, encoding?: string) =>
    localFilesService.writeFile(filePath, content, encoding)
  )

  ipcMain.handle('localFiles:createDirectory', (_, dirPath: string, name: string) =>
    localFilesService.createDirectory(dirPath, name)
  )

  ipcMain.handle('localFiles:createFile', (_, dirPath: string, name: string) =>
    localFilesService.createFile(dirPath, name)
  )

  ipcMain.handle('localFiles:copyPath', (_, sourcePath: string, destinationPath: string) =>
    localFilesService.copyPath(sourcePath, destinationPath)
  )

  ipcMain.handle('localFiles:movePath', (_, sourcePath: string, destinationPath: string) =>
    localFilesService.movePath(sourcePath, destinationPath)
  )

  ipcMain.handle('localFiles:renamePath', (_, targetPath: string, newName: string) =>
    localFilesService.renamePath(targetPath, newName)
  )

  ipcMain.handle('localFiles:deletePath', (_, targetPath: string) =>
    localFilesService.deletePath(targetPath)
  )

  ipcMain.handle('localFiles:changePermissions', (_, targetPath: string, options: PermissionChangeOptions) =>
    localFilesService.changePermissions(targetPath, options)
  )

  ipcMain.handle('localFiles:selectFiles', async (event, defaultPath?: string) => {
    const window = BrowserWindow.fromWebContents(event.sender) ?? undefined
    const options: OpenDialogOptions = {
      defaultPath,
      properties: ['openFile', 'openDirectory', 'multiSelections', 'createDirectory']
    }
    const result = window
      ? await dialog.showOpenDialog(window, options)
      : await dialog.showOpenDialog(options)
    return result.canceled ? [] : result.filePaths
  })

  ipcMain.handle('localFiles:selectDirectory', async (event, defaultPath?: string) => {
    const window = BrowserWindow.fromWebContents(event.sender) ?? undefined
    const options: OpenDialogOptions = {
      defaultPath: defaultPath || app.getPath('downloads'),
      properties: ['openDirectory', 'createDirectory']
    }
    const result = window
      ? await dialog.showOpenDialog(window, options)
      : await dialog.showOpenDialog(options)
    return result.canceled ? null : result.filePaths[0] ?? null
  })
}
