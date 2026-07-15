import { BrowserWindow } from 'electron'
import { FileProfileRepository } from '../services/file-profile-repository.js'
import { FileSshKeyRepository } from '../services/ssh-keys/file-ssh-key-repository.js'
import { SshKeyService } from '../services/ssh-keys/ssh-key-service.js'
import { LocalFilesService } from '../services/local-files-service.js'
import {
  WorkspaceService,
  seedCommandFolders,
  seedCommandTemplates,
  seedProfiles
} from '../services/workspace-service.js'
import { TransferJournal } from '../services/transfers/transfer-journal.js'
import { registerAppHandlers } from './app-handlers.js'
import { registerLocalFilesHandlers } from './local-files-handlers.js'
import { registerRemoteFilesHandlers } from './remote-files-handlers.js'
import { registerSshInteractionHandlers } from './ssh-interaction-handlers.js'
import { registerSshKeyHandlers } from './ssh-key-handlers.js'
import { registerTerminalHandlers } from './terminal-handlers.js'
import { registerTransferHandlers } from './transfer-handlers.js'
import type { IpcServices, IpcWindowOptions } from './types.js'
import { registerWorkspaceHandlers } from './workspace-handlers.js'

export function registerIpcHandlers(userDataPath: string, options: IpcWindowOptions) {
  const profileRepository = new FileProfileRepository(
    userDataPath,
    seedProfiles,
    seedCommandTemplates,
    seedCommandFolders
  )
  const sshKeyService = new SshKeyService(new FileSshKeyRepository(userDataPath), profileRepository)
  const workspaceService = new WorkspaceService(profileRepository, {
    getLocale: () => options.getUiPreferences().locale,
    transferJournal: new TransferJournal(userDataPath),
    sshKeyService
  })
  const services: IpcServices = {
    workspaceService,
    sshKeyService,
    localFilesService: new LocalFilesService(),
    broadcastSnapshot(snapshot) {
      for (const window of BrowserWindow.getAllWindows()) {
        if (!window.isDestroyed()) {
          try {
            window.webContents.send('workspace:snapshot', snapshot)
          } catch (error) {
            if (!isIgnorableBroadcastError(error)) {
              throw error
            }
          }
        }
      }
    }
  }

  registerAppHandlers(options)
  registerWorkspaceHandlers(services, options)
  registerLocalFilesHandlers(services)
  registerTransferHandlers(services)
  registerTerminalHandlers(services)
  registerRemoteFilesHandlers(services)
  registerSshInteractionHandlers(services)
  registerSshKeyHandlers(services, options)

  return services
}

function isIgnorableBroadcastError(error: unknown) {
  if (!(error instanceof Error)) {
    return false
  }

  const errno = error as NodeJS.ErrnoException
  if (errno.code === 'EPIPE') {
    return true
  }

  return (
    error.message.includes('Render frame was disposed') ||
    error.message.includes('Object has been destroyed') ||
    error.message.includes('WebContents was destroyed')
  )
}
