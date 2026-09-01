import { useEffect, type Dispatch, type SetStateAction } from 'react'
import type { LocalFileItem, SessionSnapshot } from '@fileterm/core'
import { areStringArraysEqual } from './file-manager-utils'
import type { FileContextMenuState } from './file-manager-types'
import type { RemoteFileSortState } from './file-tables'

const VIEW_TRANSITION_LOADING_MS = 180

export type FileManagerStateSyncOptions = {
  localItems: LocalFileItem[]
  localPath: string
  localPanePath: string
  activeSession: Pick<SessionSnapshot, 'remoteFiles' | 'remotePath'>
  canUseRemoteFiles: boolean
  activeTabId: string | null
  activeView: 'file' | 'command' | 'tunnel'
  canManageTunnels: boolean
  isViewLoading: boolean
  setIsViewLoading: Dispatch<SetStateAction<boolean>>
  setLocalPathInput: Dispatch<SetStateAction<string>>
  setSelectedLocalPaths: Dispatch<SetStateAction<string[]>>
  setRemotePathInput: Dispatch<SetStateAction<string>>
  setSelectedRemotePaths: Dispatch<SetStateAction<string[]>>
  setRemoteAnchorPath: Dispatch<SetStateAction<string | null>>
  setContextMenu: Dispatch<SetStateAction<FileContextMenuState | null>>
  setRemoteSort: Dispatch<SetStateAction<RemoteFileSortState>>
  onActiveViewChange(view: 'file' | 'command' | 'tunnel'): void
}

export function useFileManagerStateSync({
  localItems,
  localPath,
  localPanePath,
  activeSession,
  canUseRemoteFiles,
  activeTabId,
  activeView,
  canManageTunnels,
  isViewLoading,
  setIsViewLoading,
  setLocalPathInput,
  setSelectedLocalPaths,
  setRemotePathInput,
  setSelectedRemotePaths,
  setRemoteAnchorPath,
  setContextMenu,
  setRemoteSort,
  onActiveViewChange
}: FileManagerStateSyncOptions) {
  useEffect(() => {
    setLocalPathInput((previous) => (previous === localPath || previous === localPanePath ? previous : localPanePath))
    setSelectedLocalPaths((previous) => {
      const next = previous.filter((selectedPath) => localItems.some((item) => item.path === selectedPath))
      return areStringArraysEqual(previous, next) ? previous : next
    })
  }, [localItems, localPanePath, localPath, setLocalPathInput, setSelectedLocalPaths])

  useEffect(() => {
    setRemotePathInput((previous) => (previous === activeSession.remotePath ? previous : activeSession.remotePath))
    setSelectedRemotePaths((previous) => {
      const next = previous.filter((selectedPath) =>
        activeSession.remoteFiles.some((item) => item.path === selectedPath)
      )
      return areStringArraysEqual(previous, next) ? previous : next
    })
  }, [activeSession.remoteFiles, activeSession.remotePath, setRemotePathInput, setSelectedRemotePaths])

  useEffect(() => {
    if (canUseRemoteFiles) return
    setSelectedRemotePaths([])
    setRemoteAnchorPath(null)
    setContextMenu((previous) => (previous?.pane === 'remote' ? null : previous))
  }, [canUseRemoteFiles, setContextMenu, setRemoteAnchorPath, setSelectedRemotePaths])

  useEffect(() => {
    setRemoteSort({ field: 'name', direction: 'asc' })
  }, [activeTabId, setRemoteSort])

  useEffect(() => {
    if (!canManageTunnels && activeView === 'tunnel') {
      onActiveViewChange('file')
    }
  }, [activeView, canManageTunnels, onActiveViewChange])

  useEffect(() => {
    if (!isViewLoading) return
    const timer = window.setTimeout(() => setIsViewLoading(false), VIEW_TRANSITION_LOADING_MS)
    return () => window.clearTimeout(timer)
  }, [activeView, isViewLoading, setIsViewLoading])
}
