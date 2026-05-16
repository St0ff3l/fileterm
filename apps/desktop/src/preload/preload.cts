const { contextBridge, ipcRenderer } = require('electron') as typeof import('electron')

import type { CreateProfileInput, LocalFileItem, WorkspaceSnapshot } from '@termdock/core'

const api = {
  platform: typeof process !== 'undefined' ? process.platform : 'unknown',
  appName: 'TermDock',
  isDesktop: true,
  getSnapshot: (): Promise<WorkspaceSnapshot> => ipcRenderer.invoke('workspace:getSnapshot'),
  createProfile: (input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:createProfile', input),
  updateProfile: (profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:updateProfile', profileId, input),
  deleteProfile: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:deleteProfile', profileId),
  openProfile: (profileId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:openProfile', profileId),
  activateTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:activateTab', tabId),
  closeTab: (tabId: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('workspace:closeTab', tabId),
  listLocalDirectory: (dirPath?: string): Promise<{ path: string, items: LocalFileItem[] }> =>
    ipcRenderer.invoke('localFiles:listDirectory', dirPath),
  queueUpload: (fileNames: string[]): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('transfer:queueUpload', fileNames),
  writeTerminal: (tabId: string, data: string): Promise<void> =>
    ipcRenderer.invoke('terminal:write', tabId, data),
  resizeTerminal: (tabId: string, cols: number, rows: number): Promise<void> =>
    ipcRenderer.invoke('terminal:resize', tabId, cols, rows),
  openRemotePath: (tabId: string, targetPath: string): Promise<WorkspaceSnapshot> =>
    ipcRenderer.invoke('remoteFiles:openPath', tabId, targetPath),
  onTerminalData: (listener: (payload: { tabId: string, chunk: string }) => void) => {
    const wrapped = (_event: unknown, payload: { tabId: string, chunk: string }) => listener(payload)
    ipcRenderer.on('terminal:data', wrapped)
    return () => ipcRenderer.off('terminal:data', wrapped)
  },
  onTerminalState: (listener: (payload: { tabId: string, summary: string, transcript: string, connected: boolean }) => void) => {
    const wrapped = (_event: unknown, payload: { tabId: string, summary: string, transcript: string, connected: boolean }) => listener(payload)
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
