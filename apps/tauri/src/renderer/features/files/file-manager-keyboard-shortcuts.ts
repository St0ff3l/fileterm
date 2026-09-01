import type { KeyboardEvent } from 'react'
import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { hasSelectedText } from '../../app/app-utils'
import type { FilePane } from './file-manager-types'

export function handleFileManagerKeyboardShortcuts(
  event: KeyboardEvent<HTMLDivElement>,
  {
    canPaste,
    keyboardPane,
    keyboardSelection,
    onClearCutState,
    onCopyItems,
    onCutItems,
    onPasteIntoPane
  }: {
    canPaste: boolean
    keyboardPane: FilePane
    keyboardSelection: Array<LocalFileItem | RemoteFileItem>
    onClearCutState(): void
    onCopyItems(pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>): void
    onCutItems(pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>): void
    onPasteIntoPane(pane: FilePane): void
  }
) {
  if (event.key === 'Escape') {
    onClearCutState()
    return
  }

  if (!(event.metaKey || event.ctrlKey) || event.altKey) {
    return
  }

  const target = event.target
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  ) {
    return
  }

  if (hasSelectedText()) {
    return
  }

  const key = event.key.toLowerCase()
  if (key === 'c' || key === 'x') {
    if (!keyboardSelection.length) {
      return
    }
    event.preventDefault()
    if (key === 'c') {
      onCopyItems(keyboardPane, keyboardSelection)
    } else {
      onCutItems(keyboardPane, keyboardSelection)
    }
    return
  }

  if (key === 'v' && canPaste) {
    event.preventDefault()
    onPasteIntoPane(keyboardPane)
  }
}
