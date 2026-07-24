import { useState } from 'react'
import type { FileTermDesktopApi } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { ContextMenu, type ContextMenuEntry } from '../common/ContextMenu'
import { t } from '../../i18n'

type WindowMenuKind = 'file' | 'view' | 'window'

interface OpenMenu {
  kind: WindowMenuKind
  position: { x: number; y: number }
}

// Windows 平台快捷键文本。WindowMenubar 仅在 Windows 渲染（见 App.tsx
// `isWindowsDesktop` 判定），故这里固定使用 Windows 风格的修饰键。
const SHORTCUT_NEW_CONNECTION = 'Ctrl+N'
const SHORTCUT_CONNECTION_MANAGER = 'Ctrl+Shift+C'
const SHORTCUT_COMMAND_MANAGER = 'Ctrl+Shift+M'
const SHORTCUT_EXIT = 'Alt+F4'
const SHORTCUT_RELOAD = 'F5'
const SHORTCUT_ACTUAL_SIZE = 'Ctrl+0'
const SHORTCUT_ZOOM_IN = 'Ctrl+Plus'
const SHORTCUT_ZOOM_OUT = 'Ctrl+-'
const SHORTCUT_TOGGLE_DEVTOOLS = 'F12'
const SHORTCUT_CLOSE_WINDOW = 'Alt+F4'

// dev 构建才显示"开发者工具"项，与 Rust 端 `#[cfg(debug_assertions)]`
// 行为一致：生产构建不暴露 devtools 入口。
const isDevBuild = Boolean(import.meta.env.DEV)

export function WindowMenubar({ desktopApi, isMaximized }: { desktopApi?: FileTermDesktopApi; isMaximized: boolean }) {
  const [openMenu, setOpenMenu] = useState<OpenMenu | null>(null)

  const openMenuAt = (kind: WindowMenuKind, target: HTMLButtonElement) => {
    const rect = target.getBoundingClientRect()
    setOpenMenu({ kind, position: { x: Math.round(rect.left), y: Math.round(rect.bottom) } })
  }

  const buildItems = (kind: WindowMenuKind): ContextMenuEntry[] => {
    if (kind === 'file') {
      return [
        {
          label: t.windowMenuNewConnection,
          shortcut: SHORTCUT_NEW_CONNECTION,
          action: () => void desktopApi?.openConnectionFormWindow('create')
        },
        {
          label: t.windowMenuConnectionManager,
          shortcut: SHORTCUT_CONNECTION_MANAGER,
          action: () => void desktopApi?.openConnectionManagerWindow()
        },
        {
          label: t.windowMenuCommandManager,
          shortcut: SHORTCUT_COMMAND_MANAGER,
          action: () => void desktopApi?.openCommandManagerWindow()
        },
        { separator: true },
        { label: t.windowMenuOpenLogs, action: () => void desktopApi?.openLogsDirectory() },
        { separator: true },
        { label: t.windowMenuExit, shortcut: SHORTCUT_EXIT, action: () => void desktopApi?.requestQuitApp() }
      ]
    }
    if (kind === 'view') {
      const items: ContextMenuEntry[] = [
        { label: t.windowMenuReload, shortcut: SHORTCUT_RELOAD, action: () => void desktopApi?.reloadCurrentWindow() }
      ]
      if (isDevBuild) {
        items.push({
          label: t.windowMenuToggleDevtools,
          shortcut: SHORTCUT_TOGGLE_DEVTOOLS,
          action: () => void desktopApi?.toggleDevtools()
        })
      }
      items.push(
        { separator: true },
        {
          label: t.windowMenuActualSize,
          shortcut: SHORTCUT_ACTUAL_SIZE,
          action: () => void desktopApi?.setWindowZoom('reset')
        },
        { label: t.windowMenuZoomIn, shortcut: SHORTCUT_ZOOM_IN, action: () => void desktopApi?.setWindowZoom('in') },
        { label: t.windowMenuZoomOut, shortcut: SHORTCUT_ZOOM_OUT, action: () => void desktopApi?.setWindowZoom('out') }
      )
      return items
    }
    return [
      { label: t.windowMinimize, action: () => void desktopApi?.minimizeCurrentWindow() },
      {
        label: isMaximized ? t.windowRestore : t.windowMaximize,
        action: () => void desktopApi?.toggleMaximizeCurrentWindow()
      },
      { separator: true },
      {
        label: t.windowMenuCloseWindow,
        shortcut: SHORTCUT_CLOSE_WINDOW,
        action: () => void desktopApi?.requestCloseCurrentWindow()
      }
    ]
  }

  return (
    <div
      className="window-menubar"
      data-tauri-drag-region="deep"
      onDoubleClick={(event) => {
        if (event.target instanceof Element && event.target.closest('button')) {
          return
        }
        void desktopApi?.toggleMaximizeCurrentWindow()
      }}
    >
      <div className="window-menu-items">
        <button type="button" onClick={(event) => openMenuAt('file', event.currentTarget)}>
          {t.nativeMenuFile}
        </button>
        <button type="button" onClick={(event) => openMenuAt('view', event.currentTarget)}>
          {t.nativeMenuView}
        </button>
        <button type="button" onClick={(event) => openMenuAt('window', event.currentTarget)}>
          {t.nativeMenuWindow}
        </button>
      </div>
      <div className="window-control-buttons">
        <button
          aria-label={t.windowMinimize}
          type="button"
          onClick={() => {
            void desktopApi?.minimizeCurrentWindow()
          }}
        >
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
        <button
          aria-label={isMaximized ? t.windowRestore : t.windowMaximize}
          type="button"
          onClick={() => {
            void desktopApi?.toggleMaximizeCurrentWindow()
          }}
        >
          {isMaximized ? (
            <svg width="10" height="10" viewBox="0 0 10 10">
              <path
                d="M1.5,3.5 L6.5,3.5 L6.5,8.5 L1.5,8.5 Z M3.5,3.5 L3.5,1.5 L8.5,1.5 L8.5,6.5 L6.5,6.5"
                fill="none"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          ) : (
            <svg width="10" height="10" viewBox="0 0 10 10">
              <rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1" />
            </svg>
          )}
        </button>
        <CloseButton
          aria-label={t.windowClose}
          onClick={() => {
            void desktopApi?.closeCurrentWindow()
          }}
          size="window"
        />
      </div>
      {openMenu ? (
        <ContextMenu
          className="window-context-menu"
          items={buildItems(openMenu.kind)}
          onClose={() => setOpenMenu(null)}
          position={openMenu.position}
        />
      ) : null}
    </div>
  )
}
