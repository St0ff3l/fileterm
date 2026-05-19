const { contextBridge, ipcRenderer, webUtils } = require('electron') as typeof import('electron')

import type {
  CommandExecutionOptions,
  CommandTemplateInput,
  ConnectionFormMode,
  CommandExecutionResult,
  CreateProfileInput,
  DirectorySnapshot,
  FileEditorWindowInput,
  LocalFileItem,
  PermissionChangeOptions,
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
  openCommandFormWindow: (mode: ConnectionFormMode, commandId?: string, folderId?: string): Promise<void> =>
    ipcRenderer.invoke('app:openCommandFormWindow', mode, commandId, folderId),
  openFileEditorWindow: (input: FileEditorWindowInput): Promise<void> =>
    ipcRenderer.invoke('app:openFileEditorWindow', input),
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
  readLocalFile: (filePath: string, encoding?: string): Promise<string> =>
    ipcRenderer.invoke('localFiles:readFile', filePath, encoding),
  writeLocalFile: (filePath: string, content: string, encoding?: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:writeFile', filePath, content, encoding),
  createLocalDirectory: (dirPath: string, name: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:createDirectory', dirPath, name),
  createLocalFile: (dirPath: string, name: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:createFile', dirPath, name),
  renameLocalPath: (targetPath: string, newName: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:renamePath', targetPath, newName),
  deleteLocalPath: (targetPath: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:deletePath', targetPath),
  changeLocalPermissions: (targetPath: string, options: PermissionChangeOptions): Promise<void> =>
    ipcRenderer.invoke('localFiles:changePermissions', targetPath, options),
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
  readRemoteFile: (tabId: string, targetPath: string, encoding?: string): Promise<string> =>
    ipcRenderer.invoke('remoteFiles:readFile', tabId, targetPath, encoding),
  writeRemoteFile: (tabId: string, targetPath: string, content: string, encoding?: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:writeFile', tabId, targetPath, content, encoding),
  createRemoteDirectory: (tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:createDirectory', tabId, parentPath, name),
  createRemoteFile: (tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:createFile', tabId, parentPath, name),
  renameRemotePath: (tabId: string, targetPath: string, newName: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:renamePath', tabId, targetPath, newName),
  deleteRemotePath: (tabId: string, targetPath: string, targetType: 'file' | 'folder'): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:deletePath', tabId, targetPath, targetType),
  changeRemotePermissions: (tabId: string, targetPath: string, options: PermissionChangeOptions): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:changePermissions', tabId, targetPath, options),
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
