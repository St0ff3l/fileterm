import { ContextMenu, type ContextMenuEntry } from '../features/common/context-menu'
import { t } from '../i18n'
import type { SplitPaneDirection } from './terminal-view-utils'

type TerminalContextMenuProps = {
  position: { x: number; y: number }
  selectedText: string
  hasSelection: boolean
  shortcuts: {
    copy: string
    paste: string
    find: string
    vertical: string
    horizontal: string
    closePane: string
  }
  hasSplitPane: boolean
  hasCloseablePane: boolean
  setContextMenu(position: { x: number; y: number } | null): void
  runCopy(): void
  runPaste(): Promise<void>
  runPasteSelection(selectedText: string): void
  runFind(): void
  runSaveSessionLog(): Promise<void>
  runClear(): void
  runSplitPane(direction: SplitPaneDirection): void
  runClosePane(): void
}

export function TerminalContextMenu({
  position,
  selectedText,
  hasSelection,
  shortcuts,
  hasSplitPane,
  hasCloseablePane,
  setContextMenu,
  runCopy,
  runPaste,
  runPasteSelection,
  runFind,
  runSaveSessionLog,
  runClear,
  runSplitPane,
  runClosePane
}: TerminalContextMenuProps) {
  const items: ContextMenuEntry[] = [
    { label: t.copy, shortcut: shortcuts.copy, disabled: !hasSelection, action: runCopy },
    { label: t.paste, shortcut: shortcuts.paste, action: () => void runPaste() },
    {
      label: t.pasteSelectedText,
      disabled: !selectedText,
      action: () => runPasteSelection(selectedText)
    },
    ...(hasSplitPane
      ? [
          { separator: true },
          {
            label: t.splitVertically,
            shortcut: shortcuts.vertical,
            action: () => runSplitPane('row')
          },
          {
            label: t.splitHorizontally,
            shortcut: shortcuts.horizontal,
            action: () => runSplitPane('column')
          },
          // 只在分屏中且还有兄弟 pane 时显示：单 pane 时关闭等价于关 tab，
          // 走平台关闭键（Cmd+W / Ctrl+Shift+W）的确认流程更合适。
          ...(hasCloseablePane
            ? [
                {
                  label: t.closePane,
                  shortcut: shortcuts.closePane,
                  action: runClosePane
                }
              ]
            : [])
        ]
      : []),
    { separator: true },
    { label: t.find, shortcut: shortcuts.find, action: runFind },
    { separator: true },
    { label: t.saveSessionLog, action: () => void runSaveSessionLog() },
    { label: t.clearScreen, action: runClear }
  ]

  return (
    <ContextMenu
      autoFocus={false}
      className="terminal-context-menu"
      items={items}
      onClose={() => setContextMenu(null)}
      position={position}
    />
  )
}
