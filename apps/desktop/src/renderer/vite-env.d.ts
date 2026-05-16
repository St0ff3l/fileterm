/// <reference types="vite/client" />
import type { CreateProfileInput, LocalFileItem, WorkspaceSnapshot } from '@termdock/core'

declare global {
  interface Window {
    termdock?: {
      platform: string
      appName: string
      isDesktop: boolean
      getSnapshot(): Promise<WorkspaceSnapshot>
      createProfile(input: CreateProfileInput): Promise<WorkspaceSnapshot>
      updateProfile(profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot>
      deleteProfile(profileId: string): Promise<WorkspaceSnapshot>
      openProfile(profileId: string): Promise<WorkspaceSnapshot>
      activateTab(tabId: string): Promise<WorkspaceSnapshot>
      closeTab(tabId: string): Promise<WorkspaceSnapshot>
      listLocalDirectory(dirPath?: string): Promise<{ path: string, items: LocalFileItem[] }>
      queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot>
      writeTerminal(tabId: string, data: string): Promise<void>
      resizeTerminal(tabId: string, cols: number, rows: number): Promise<void>
      openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot>
      onTerminalData(listener: (payload: { tabId: string, chunk: string }) => void): () => void
      onTerminalState(listener: (payload: { tabId: string, summary: string, transcript: string, connected: boolean }) => void): () => void
      onWorkspaceSnapshot(listener: (snapshot: WorkspaceSnapshot) => void): () => void
    }
  }
}

export {}
