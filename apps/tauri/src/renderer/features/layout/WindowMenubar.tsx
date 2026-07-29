import { useState } from 'react'
import type { FileTermDesktopApi } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { ContextMenu, type ContextMenuEntry } from '../common/ContextMenu'
import { t } from '../../i18n'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'

type WindowMenuKind = 'file' | 'view' | 'window'

interface OpenMenu {
  kind: WindowMenuKind
  position: { x: number; y: number }
}

// Windows/Linux 共用自绘菜单栏（见 App.tsx 的 `usesCustomWindowChrome`
// 判定）。窗口动作只保留 Alt+F4；终端字号快捷键交给最后聚焦的 xterm，
// 不占用 WebView 页面缩放。
const SHORTCUT_EXIT = 'Alt+F4'
const SHORTCUT_CLOSE_WINDOW = 'Alt+F4'
const SHORTCUT_TERMINAL_ZOOM_IN = 'Ctrl+Shift++'
const SHORTCUT_TERMINAL_ZOOM_OUT = 'Ctrl+Shift+-'
const SHORTCUT_TERMINAL_ZOOM_RESET = 'Ctrl+0'

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
          action: () => void desktopApi?.openConnectionFormWindow('create')
        },
        {
          label: t.windowMenuConnectionManager,
          action: () => void desktopApi?.openConnectionManagerWindow()
        },
        {
          label: t.windowMenuCommandManager,
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
        { label: t.windowMenuReload, action: () => void desktopApi?.reloadCurrentWindow() }
      ]
      if (isDevBuild) {
        items.push({
          label: t.windowMenuToggleDevtools,
          action: () => void desktopApi?.toggleDevtools()
        })
      }
      items.push(
        { separator: true },
        {
          label: t.terminalZoomIn,
          shortcut: SHORTCUT_TERMINAL_ZOOM_IN,
          action: () => dispatchAppEvent(APP_EVENT.terminalZoom, 'in')
        },
        {
          label: t.terminalZoomOut,
          shortcut: SHORTCUT_TERMINAL_ZOOM_OUT,
          action: () => dispatchAppEvent(APP_EVENT.terminalZoom, 'out')
        },
        {
          label: t.terminalZoomReset,
          shortcut: SHORTCUT_TERMINAL_ZOOM_RESET,
          action: () => dispatchAppEvent(APP_EVENT.terminalZoom, 'reset')
        }
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
        <button
          aria-expanded={openMenu?.kind === 'file'}
          aria-haspopup="menu"
          className={openMenu?.kind === 'file' ? 'is-open' : undefined}
          type="button"
          onClick={(event) => openMenuAt('file', event.currentTarget)}
        >
          {t.nativeMenuFile}
        </button>
        <button
          aria-expanded={openMenu?.kind === 'view'}
          aria-haspopup="menu"
          className={openMenu?.kind === 'view' ? 'is-open' : undefined}
          type="button"
          onClick={(event) => openMenuAt('view', event.currentTarget)}
        >
          {t.nativeMenuView}
        </button>
        <button
          aria-expanded={openMenu?.kind === 'window'}
          aria-haspopup="menu"
          className={openMenu?.kind === 'window' ? 'is-open' : undefined}
          type="button"
          onClick={(event) => openMenuAt('window', event.currentTarget)}
        >
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
