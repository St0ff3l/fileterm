import { BrowserWindow, ipcMain } from 'electron'
import type { ImportSshKeyInput, SshKeyMetadata } from '@fileterm/core'
import type { IpcServices, IpcWindowOptions } from './types.js'
import { resolveWindow } from './types.js'

export function registerSshKeyHandlers(services: IpcServices, options: IpcWindowOptions) {
  const broadcast = async () => {
    const keys = await services.sshKeyService.list()
    for (const window of BrowserWindow.getAllWindows()) {
      if (!window.isDestroyed()) {
        window.webContents.send('sshKeys:changed', keys)
      }
    }
    return keys
  }

  ipcMain.handle('sshKeys:list', () => services.sshKeyService.list())

  ipcMain.handle('sshKeys:import', async (event, input?: ImportSshKeyInput) => {
    const result = await services.sshKeyService.import(input, resolveWindow(event.sender, options))
    if (result) {
      await broadcast()
    }
    return result
  })

  ipcMain.handle('sshKeys:updateNote', async (_, keyId: string, note: string) => {
    const updated = await services.sshKeyService.updateNote(keyId, note)
    await broadcast()
    return updated
  })

  ipcMain.handle('sshKeys:delete', async (_, keyId: string) => {
    await services.sshKeyService.delete(keyId)
    await broadcast()
  })
}

export type SshKeysChangedPayload = SshKeyMetadata[]
