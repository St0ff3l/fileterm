const { contextBridge, ipcRenderer, webUtils } = require('electron') as typeof import('electron')

import type {
  CommandExecutionOptions,
  CommandTemplateInput,
  CommandFolder,
  ConnectionFormMode,
  ConnectionFolder,
  CommandExecutionResult,
  CreateProfileInput,
  DirectorySnapshot,
  FileEditorWindowInput,
  LocalFileItem,
  PermissionChangeOptions,
  RemoteFileAccessOptions,
  SshInteractionRequest,
  SshInteractionResponse,
  TransferTargetOptions,
  TermdockDesktopApi,
  TerminalDataPayload,
  TerminalStatePayload,
  WorkspaceSnapshot
} from '@termdock/core'

const api: TermdockDesktopApi = {
  platform: typeof process !== 'undefined' ? process.platform : 'unknown',
  appName: 'TermDock',
  isDesktop: true,
  readClipboardText: (): Promise<string> => ipcRenderer.invoke('app:readClipboardText'),
  writeClipboardText: (text: string): Promise<void> => ipcRenderer.invoke('app:writeClipboardText', text),
  getUiPreferences: () => ipcRenderer.invoke('app:getUiPreferences'),
  setUiPreferences: (input) => ipcRenderer.invoke('app:setUiPreferences', input),
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
  openExternalUrl: (url: string): Promise<void> =>
    ipcRenderer.invoke('app:openExternalUrl', url),
  openLogsDirectory: (): Promise<void> =>
    ipcRenderer.invoke('app:openLogsDirectory'),
  minimizeCurrentWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:minimizeCurrentWindow'),
  toggleMaximizeCurrentWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:toggleMaximizeCurrentWindow'),
  closeCurrentWindow: (): Promise<void> =>
    ipcRenderer.invoke('app:closeCurrentWindow'),
  requestQuitApp: (): Promise<void> =>
    ipcRenderer.invoke('app:requestQuitApp'),
  getSnapshot: (): Promise<WorkspaceSnapshot> => ipcRenderer.invoke('workspace:getSnapshot'),
  createProfile: (input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createProfile', input),
  updateProfile: (profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateProfile', profileId, input),
  deleteProfile: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteProfile', profileId),
  createFolder: (name: string, parentId?: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createFolder', name, parentId),
  updateFolder: (folderId: string, updates: Partial<ConnectionFolder>): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateFolder', folderId, updates),
  deleteFolder: (folderId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteFolder', folderId),
  updateEntityOrder: (id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateEntityOrder', id, newParentId, newOrder),
  createCommandFolder: (name: string, parentId?: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createCommandFolder', name, parentId),
  updateCommandFolder: (folderId: string, updates: Partial<CommandFolder>): Promise<WorkspaceSnapshot> =>
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
  copyLocalPath: (sourcePath: string, destinationPath: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:copyPath', sourcePath, destinationPath),
  moveLocalPath: (sourcePath: string, destinationPath: string): Promise<void> =>
    ipcRenderer.invoke('localFiles:movePath', sourcePath, destinationPath),
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
  clearTransfers: (transferIds: string[]): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:clear', transferIds),
  uploadFile: (tabId: string, localPath: string, remoteDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:uploadFile', tabId, localPath, remoteDirectory, options),
  downloadFile: (tabId: string, remotePath: string, localDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:downloadFile', tabId, remotePath, localDirectory, options),
  downloadRemotePath: (tabId: string, remotePath: string, targetType: 'file' | 'folder', localDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:downloadRemotePath', tabId, remotePath, targetType, localDirectory, options),
  setRemoteFileAccessMode: (tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:setFileAccessMode', tabId, mode, options),
  writeTerminal: (tabId: string, data: string): Promise<void> =>
    ipcRenderer.invoke('terminal:write', tabId, data),
  resizeTerminal: (tabId: string, cols: number, rows: number, width: number, height: number): Promise<void> =>
    ipcRenderer.invoke('terminal:resize', tabId, cols, rows, width, height),
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
  copyRemotePath: (tabId: string, targetPath: string, destinationPath: string, targetType: 'file' | 'folder'): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:copyPath', tabId, targetPath, destinationPath, targetType),
  moveRemotePath: (tabId: string, targetPath: string, destinationPath: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:movePath', tabId, targetPath, destinationPath),
  renameRemotePath: (tabId: string, targetPath: string, newName: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:renamePath', tabId, targetPath, newName),
  deleteRemotePath: (tabId: string, targetPath: string, targetType: 'file' | 'folder'): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:deletePath', tabId, targetPath, targetType),
  resolveSshInteraction: (requestId: string, response: SshInteractionResponse): Promise<void> =>
    ipcRenderer.invoke('ssh:resolveInteraction', requestId, response),
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
  },
  onSshInteraction: (listener: (request: SshInteractionRequest) => void) => {
    const wrapped = (_event: unknown, request: SshInteractionRequest) => listener(request)
    ipcRenderer.on('ssh:interaction', wrapped)
    return () => ipcRenderer.off('ssh:interaction', wrapped)
  },
  onWindowCloseRequest: (listener: (event: { isQuit: boolean }) => void) => {
    const wrapped = (_event: unknown, data: { isQuit: boolean }) => listener(data)
    ipcRenderer.on('app:window-close-request', wrapped)
    return () => ipcRenderer.off('app:window-close-request', wrapped)
  },
  onRequestCloseActiveWorkspaceItem: (listener: () => void) => {
    const wrapped = () => listener()
    ipcRenderer.on('app:close-active-workspace-item-request', wrapped)
    return () => ipcRenderer.off('app:close-active-workspace-item-request', wrapped)
  },
  confirmCloseWindow: (action: 'quit' | 'hide' | 'cancel'): Promise<void> =>
    ipcRenderer.invoke('app:confirmCloseWindow', action)
}

try {
  contextBridge.exposeInMainWorld('termdock', api)
} catch (error) {
  console.error('Failed to expose preload API', error)
}
