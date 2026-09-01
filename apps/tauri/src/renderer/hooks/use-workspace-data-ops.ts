import {
  type CommandTemplateInput,
  type ConnectionFolder,
  type FileTermDesktopApi,
  type WorkspaceSnapshot
} from '@fileterm/core'
import { useRef } from 'react'

interface UseWorkspaceDataOpsParams {
  desktopApi: FileTermDesktopApi | null
  isCommandFormWindow: boolean
  onApplySnapshot: (snapshot: WorkspaceSnapshot) => void
  onBusyChange: (busy: boolean) => void
  onError: (scope: string, err: unknown) => void
  onCloseCurrentWindow: () => void
}

export function useWorkspaceDataOps({
  desktopApi,
  isCommandFormWindow,
  onApplySnapshot,
  onBusyChange,
  onError,
  onCloseCurrentWindow
}: UseWorkspaceDataOpsParams) {
  const pendingOperationCountRef = useRef(0)

  const withAtomicOp = async ({
    scope,
    action,
    onAfter
  }: {
    scope: string
    action: () => Promise<WorkspaceSnapshot>
    onAfter?: () => void
  }) => {
    if (!desktopApi) {
      return false
    }

    try {
      pendingOperationCountRef.current += 1
      if (pendingOperationCountRef.current === 1) {
        onBusyChange(true)
      }
      const snapshot = await action()
      onApplySnapshot(snapshot)
      onAfter?.()
      return true
    } catch (err) {
      onError(scope, err)
      return false
    } finally {
      pendingOperationCountRef.current = Math.max(0, pendingOperationCountRef.current - 1)
      if (pendingOperationCountRef.current === 0) {
        onBusyChange(false)
      }
    }
  }

  const saveCommandTemplate = (commandId: string | null, input: CommandTemplateInput) =>
    withAtomicOp({
      scope: '保存命令模板',
      action: () =>
        commandId ? desktopApi!.updateCommandTemplate(commandId, input) : desktopApi!.createCommandTemplate(input),
      onAfter: isCommandFormWindow ? onCloseCurrentWindow : undefined
    })

  const createCommandFolder = (name: string) =>
    withAtomicOp({
      scope: '新建命令分类',
      action: () => desktopApi!.createCommandFolder(name)
    })

  const updateCommandFolder = (folderId: string, updates: { name?: string; parentId?: string; order?: number }) =>
    withAtomicOp({
      scope: '更新命令分类',
      action: () => desktopApi!.updateCommandFolder(folderId, updates)
    })

  const updateCommandOrder = (id: string, parentId: string | undefined, order: number) =>
    withAtomicOp({
      scope: '调整命令顺序',
      action: () => desktopApi!.updateCommandOrder(id, parentId, order)
    })

  const deleteCommandFolder = (folderId: string) =>
    withAtomicOp({
      scope: '删除命令分类',
      action: () => desktopApi!.deleteCommandFolder(folderId)
    })

  const deleteCommandTemplate = (commandId: string) =>
    withAtomicOp({
      scope: '删除命令模板',
      action: () => desktopApi!.deleteCommandTemplate(commandId)
    })

  const createConnectionFolder = (name: string) =>
    withAtomicOp({
      scope: '新建连接分类',
      action: () => desktopApi!.createFolder(name)
    })

  const updateConnectionFolder = (folderId: string, updates: Partial<ConnectionFolder>) =>
    withAtomicOp({
      scope: '更新连接分类',
      action: () => desktopApi!.updateFolder(folderId, updates)
    })

  const deleteConnectionFolder = (folderId: string) =>
    withAtomicOp({
      scope: '删除连接分类',
      action: () => desktopApi!.deleteFolder(folderId)
    })

  const updateConnectionOrder = (id: string, newParentId: string | undefined, newOrder: number) =>
    withAtomicOp({
      scope: '调整连接顺序',
      action: () => desktopApi!.updateEntityOrder(id, newParentId, newOrder)
    })

  return {
    saveCommandTemplate,
    createCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    deleteCommandFolder,
    deleteCommandTemplate,
    createConnectionFolder,
    updateConnectionFolder,
    deleteConnectionFolder,
    updateConnectionOrder
  }
}

export type UseWorkspaceDataOpsResult = ReturnType<typeof useWorkspaceDataOps>
