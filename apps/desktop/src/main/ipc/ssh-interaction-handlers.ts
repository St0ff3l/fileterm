import { ipcMain } from 'electron'
import type { SshInteractionResponse } from '@fileterm/core'
import type { IpcServices } from './types.js'

export function registerSshInteractionHandlers(services: IpcServices) {
  const { workspaceService } = services

  ipcMain.handle('ssh:resolveInteraction', async (_, requestId: string, response: SshInteractionResponse) => {
    await workspaceService.resolveSshInteraction(requestId, response)
  })
}
