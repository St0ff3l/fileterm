import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import type { AppIconName } from '../common/app-icon'

export type FilePane = 'local' | 'remote'

export type InternalFileDrag = {
  sourcePane: FilePane
  items: Array<LocalFileItem | RemoteFileItem>
  startX: number
  startY: number
  pointerId: number
  active: boolean
}

export type InternalFileDragPreview = {
  names: string[]
  icon: AppIconName
  x: number
  y: number
}

export type FileContextMenuState = {
  pane: FilePane
  x: number
  y: number
  path: string | null
}

export type StringListSetter = Dispatch<SetStateAction<string[]>>

export type BooleanRef = MutableRefObject<boolean>
