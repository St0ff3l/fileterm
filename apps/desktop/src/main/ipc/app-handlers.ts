import { BrowserWindow, ipcMain } from 'electron'
import type { ConnectionFormMode, FileEditorWindowInput } from '@termdock/core'
import type { IpcWindowOptions } from './types.js'

export function registerAppHandlers(options: IpcWindowOptions) {
  ipcMain.handle('app:getUiPreferences', () => options.getUiPreferences())

  ipcMain.handle('app:setUiPreferences', (_event, input: Partial<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>) => {
    return options.setUiPreferences(input)
  })

  ipcMain.handle('app:openConnectionManagerWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openConnectionManagerWindow(senderWindow)
    }
  })

  ipcMain.handle('app:openCommandManagerWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openCommandManagerWindow(senderWindow)
    }
  })

  ipcMain.handle('app:openConnectionFormWindow', (event, mode: ConnectionFormMode, profileId?: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openConnectionFormWindow(senderWindow, mode, profileId)
    }
  })

  ipcMain.handle('app:openCommandFormWindow', (event, mode: ConnectionFormMode, commandId?: string, folderId?: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openCommandFormWindow(senderWindow, mode, commandId, folderId)
    }
  })

  ipcMain.handle('app:openFileEditorWindow', (event, input: FileEditorWindowInput) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openFileEditorWindow(senderWindow, input)
    }
  })

  ipcMain.handle('app:openLogsDirectory', () => options.openLogsDirectory())

  ipcMain.handle('app:minimizeCurrentWindow', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.handle('app:toggleMaximizeCurrentWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    if (!senderWindow) {
      return
    }
    if (senderWindow.isMaximized()) {
      senderWindow.unmaximize()
      return
    }
    senderWindow.maximize()
  })

  ipcMain.handle('app:closeCurrentWindow', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('app:confirmCloseWindow', (_event, action: 'quit' | 'hide' | 'cancel') => {
    options.confirmCloseWindow(action)
  })
}
