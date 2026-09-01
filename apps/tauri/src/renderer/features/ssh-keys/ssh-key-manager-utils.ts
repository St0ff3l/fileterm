import type { SshKeyMetadata } from '@fileterm/core'
import type { ReactNode } from 'react'
import type { ManagerDropPosition } from '../common/manager-drag'

export const SSH_KEY_MANAGER_UI_STATE = 'ssh-key-manager-ui'
export const ROOT_DROP_TARGET_ID = '__ssh-key-root__'

export type SshKeyFolder = {
  id: string
  name: string
}

export type SshKeyManagerUiState = {
  folders: SshKeyFolder[]
  assignments: Record<string, string>
  itemOrder?: Record<string, number>
  keyOrder?: Record<string, number>
}

export type DeleteTarget = { kind: 'folder' | 'key'; id: string; name: string }
export type DragItem = { kind: 'folder' | 'key'; id: string }
export type DragPosition = ManagerDropPosition
export type SortableItem = { kind: DragItem['kind']; id: string; fallbackOrder: number }
export type DragOverState = { id: string; kind: DragItem['kind']; position: DragPosition }

export function readDraggedItem(value: string): DragItem | null {
  const match = /^fileterm-ssh-key:(folder|key):(.+)$/.exec(value)
  return match ? { kind: match[1] as DragItem['kind'], id: match[2] } : null
}

export function isSshKeyFolder(value: unknown): value is SshKeyFolder {
  return Boolean(
    value &&
    typeof value === 'object' &&
    typeof (value as SshKeyFolder).id === 'string' &&
    typeof (value as SshKeyFolder).name === 'string'
  )
}

export function createId(prefix: string) {
  return globalThis.crypto?.randomUUID?.() ?? `${prefix}-${Date.now()}`
}

export function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 34 ? `${fingerprint.slice(0, 18)}…${fingerprint.slice(-12)}` : fingerprint
}

export type RenderKeyRow = (key: SshKeyMetadata, className?: string) => ReactNode
