const { contextBridge, ipcRenderer, webUtils } = require('electron') as typeof import('electron')

import type {
  CommandExecutionOptions,
  CommandTemplateInput,
  ConnectionFormMode,
  CommandExecutionResult,
  CreateProfileInput,
  DirectorySnapshot,
  LocalFileItem,
  TermdockDesktopApi,
  TerminalDataPayload,
  TerminalStatePayload,
  WorkspaceSnapshot
} from '@termdock/core'

const api: TermdockDesktopApi = {
  platform: typeof process !== 'undefined' ? process.platform : 'unknown',
  appName: 'TermDock',
  isDesktop: true,
  openConnectionManagerWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:openConnectionManagerWindow'),
  openCommandManagerWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:openCommandManagerWindow'),
  openConnectionFormWindow: (mode: ConnectionFormMode, profileId?: string): Promise<void> =>
    ipcRenderer.invoke('app:openConnectionFormWindow', mode, profileId),
  closeCurrentWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:closeCurrentWindow'),
  getSnapshot: (): Promise<WorkspaceSnapshot> => ipcRenderer.invoke('workspace:getSnapshot'),
  createProfile: (input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createProfile', input),
  updateProfile: (profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateProfile', profileId, input),
  deleteProfile: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteProfile', profileId),
  createFolder: (name: string, parentId?: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createFolder', name, parentId),
  updateFolder: (folderId: string, updates: any): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateFolder', folderId, updates),
  deleteFolder: (folderId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteFolder', folderId),
  updateEntityOrder: (id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateEntityOrder', id, newParentId, newOrder),
  createCommandFolder: (name: string, parentId?: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createCommandFolder', name, parentId),
  updateCommandFolder: (folderId: string, updates: any): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateCommandFolder', folderId, updates),
  deleteCommandFolder: (folderId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteCommandFolder', folderId),
  updateCommandOrder: (id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateCommandOrder', id, newParentId, newOrder),
  createCommandTemplate: (input: CommandTemplateInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createCommandTemplate', input),
  updateCommandTemplate: (commandId: string, input: CommandTemplateInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateCommandTemplate', commandId, input),
  deleteCommandTemplate: (commandId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteCommandTemplate', commandId),
  executeCommandTemplate: (tabId: string, commandId: string, args?: string[], options?: CommandExecutionOptions): Promise<CommandExecutionResult> =>
    ipcRenderer.invoke('workspace:executeCommandTemplate', tabId, commandId, args, options),
  openProfile: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:openProfile', profileId),
  openProfileFromManager: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:openProfileFromManager', profileId),
  activateTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:activateTab', tabId),
  reconnectTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:reconnectTab', tabId),
  disconnectTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:disconnectTab', tabId),
  closeTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:closeTab', tabId),
  listLocalDirectory: (dirPath?: string): Promise<DirectorySnapshot<LocalFileItem>> =>
    ipcRenderer.invoke('localFiles:listDirectory', dirPath),
  readLocalFile: (filePath: string): Promise<string> =>
    ipcRenderer.invoke('localFiles:readFile', filePath),
  writeLocalFile: (filePath: string, content: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:writeFile', filePath, content),
  getDroppedFilePaths: (files: File[]): string[] =>
    files.map((file) => webUtils.getPathForFile(file)).filter(Boolean),
  selectLocalFiles: (defaultPath?: string): Promise<string[]> =>
    ipcRenderer.invoke('localFiles:selectFiles', defaultPath),
  selectLocalDirectory: (defaultPath?: string): Promise<string | null> =>
    ipcRenderer.invoke('localFiles:selectDirectory', defaultPath),
  queueUpload: (fileNames: string[]): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:queueUpload', fileNames),
  cancelTransfer: (transferId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:cancel', transferId),
  uploadFile: (tabId: string, localPath: string, remoteDirectory: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:uploadFile', tabId, localPath, remoteDirectory),
  downloadFile: (tabId: string, remotePath: string, localDirectory: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:downloadFile', tabId, remotePath, localDirectory),
  writeTerminal: (tabId: string, data: string): Promise<void> =>
    ipcRenderer.invoke('terminal:write', tabId, data),
  resizeTerminal: (tabId: string, cols: number, rows: number): Promise<void> =>
    ipcRenderer.invoke('terminal:resize', tabId, cols, rows),
  openRemotePath: (tabId: string, targetPath: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:openPath', tabId, targetPath),
  readRemoteFile: (tabId: string, targetPath: string): Promise<string> =>
    ipcRenderer.invoke('remoteFiles:readFile', tabId, targetPath),
  writeRemoteFile: (tabId: string, targetPath: string, content: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:writeFile', tabId, targetPath, content),
  onTerminalData: (listener: (payload: TerminalDataPayload) => void) => {
    const wrapped = (_event: unknown, payload: TerminalDataPayload) => listener(payload)
    ipcRenderer.on('terminal:data', wrapped)
    return () => ipcRenderer.off('terminal:data', wrapped)
  },
  onTerminalState: (listener: (payload: TerminalStatePayload) => void) => {
    const wrapped = (_event: unknown, payload: TerminalStatePayload) => listener(payload)
    ipcRenderer.on('terminal:state', wrapped)
    return () => ipcRenderer.off('terminal:state', wrapped)
  },
  onWorkspaceSnapshot: (listener: (snapshot: WorkspaceSnapshot) => void) => {
    const wrapped = (_event: unknown, snapshot: WorkspaceSnapshot) => listener(snapshot)
    ipcRenderer.on('workspace:snapshot', wrapped)
    return () => ipcRenderer.off('workspace:snapshot', wrapped)
  }
}

try {
  contextBridge.exposeInMainWorld('termdock', api)
} catch (error) {
  console.error('Failed to expose preload API', error)
}
