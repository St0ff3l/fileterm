import { clipboard, ipcMain } from 'electron'
import { appError } from '../services/app-logger.js'
import type { IpcServices } from './types.js'

export function registerTerminalHandlers(services: IpcServices) {
  const { workspaceService } = services

  ipcMain.handle('app:readClipboardText', () => clipboard.readText())

  ipcMain.handle('app:writeClipboardText', (_event, text: string) => {
    clipboard.writeText(text)
  })

  ipcMain.on('terminal:write', (_, tabId: string, data: string) => {
    void workspaceService.writeToTerminal(tabId, data).catch((error) => {
      appError('[FileTerm][Terminal] Failed to write input', error)
    })
  })

  ipcMain.on('terminal:resize', (_, tabId: string, cols: number, rows: number, width: number, height: number) => {
    void workspaceService.resizeTerminal(tabId, cols, rows, width, height).catch((error) => {
      appError('[FileTerm][Terminal] Failed to resize terminal', error)
    })
  })
}
