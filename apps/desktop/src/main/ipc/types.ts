import { BrowserWindow } from 'electron'
import type { WebContents } from 'electron'
import type { LocalFilesService } from '../services/local-files-service.js'
import type { WorkspaceService } from '../services/workspace-service.js'

export interface IpcWindowOptions {
  getMainWindow(): BrowserWindow | null
  getUiPreferences(): { theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }
  setUiPreferences(input: Partial<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>): { theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }
  openConnectionManagerWindow(parent: BrowserWindow): void
  openCommandManagerWindow(parent: BrowserWindow): void
  openConnectionFormWindow(parent: BrowserWindow, mode: 'create' | 'edit', profileId?: string): void
  openCommandFormWindow(parent: BrowserWindow, mode: 'create' | 'edit', commandId?: string, folderId?: string): void
  openFileEditorWindow(parent: BrowserWindow, input: { source: 'local' | 'remote'; path: string; name: string; tabId?: string; encoding?: string }): void
  openLogsDirectory(): Promise<void>
  confirmCloseWindow(action: 'quit' | 'hide' | 'cancel'): void
}

export interface IpcServices {
  workspaceService: WorkspaceService
  localFilesService: LocalFilesService
  broadcastSnapshot(snapshot: Awaited<ReturnType<WorkspaceService['getSnapshot']>>): void
}

export function resolveWindow(sender: WebContents, options: IpcWindowOptions) {
  return BrowserWindow.fromWebContents(sender) ?? options.getMainWindow() ?? undefined
}
