import { useState } from 'react'
import type { FileTermDesktopApi } from '@fileterm/core'
import { CloseButton } from '../common/close-button'
import { AppIcon } from '../common/app-icon'
import { ContextMenu, type ContextMenuEntry } from '../common/context-menu'
import { t } from '../../i18n'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import './window-menubar.css'

type WindowMenuKind = 'file' | 'view' | 'window'

interface OpenMenu {
  kind: WindowMenuKind
  position: { x: number; y: number }
}

// Windows/Linux 共用自绘菜单栏（见 app.tsx 的 `usesCustomWindowChrome`
// 判定）。窗口动作只保留 Alt+F4；终端缩放快捷键仍由最后聚焦的 xterm 处理。
const SHORTCUT_EXIT = 'Alt+F4'
const SHORTCUT_CLOSE_WINDOW = 'Alt+F4'
const SHORTCUT_TERMINAL_ZOOM_IN = 'Ctrl+Shift++'
const SHORTCUT_TERMINAL_ZOOM_OUT = 'Ctrl+Shift+-'
const SHORTCUT_TERMINAL_ZOOM_RESET = 'Ctrl+0'
type UiZoomPercentOperation = 'in' | 'out' | 'reset'

// dev 构建才显示"开发者工具"项，与 Rust 端 `#[cfg(debug_assertions)]`
// 行为一致：生产构建不暴露 devtools 入口。
const isDevBuild = Boolean(import.meta.env.DEV)

export function WindowMenubar({
  desktopApi,
  isMaximized,
  terminalZoomLocked,
  uiZoomLocked,
  onUiZoomPercentChange,
  onToggleUiZoomLock,
  onToggleTerminalZoomLock
}: {
  desktopApi?: FileTermDesktopApi
  isMaximized: boolean
  terminalZoomLocked: boolean
  uiZoomLocked: boolean
  onUiZoomPercentChange(operation: UiZoomPercentOperation): void
  onToggleUiZoomLock(): void
  onToggleTerminalZoomLock(): void
}) {
  const [openMenu, setOpenMenu] = useState<OpenMenu | null>(null)
  const isDesktop = desktopApi?.isDesktop !== false

  const openMenuAt = (kind: WindowMenuKind, target: HTMLButtonElement) => {
    const rect = target.getBoundingClientRect()
    setOpenMenu({ kind, position: { x: Math.round(rect.left), y: Math.round(rect.bottom) } })
  }

  const buildItems = (kind: WindowMenuKind): ContextMenuEntry[] => {
    if (kind === 'file') {
      const items: ContextMenuEntry[] = [
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
        { label: t.windowMenuOpenLogs, action: () => void desktopApi?.openLogsDirectory() }
      ]
      if (isDesktop) {
        items.push(
          { separator: true },
          { label: t.windowMenuExit, shortcut: SHORTCUT_EXIT, action: () => void desktopApi?.requestQuitApp() }
        )
      }
      return items
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
          label: t.uiZoomIn,
          disabled: uiZoomLocked,
          action: () => onUiZoomPercentChange('in')
        },
        {
          label: t.uiZoomOut,
          disabled: uiZoomLocked,
          action: () => onUiZoomPercentChange('out')
        },
        {
          label: t.uiZoomReset,
          disabled: uiZoomLocked,
          action: () => onUiZoomPercentChange('reset')
        },
        {
          label: t.lockUiZoom,
          checked: uiZoomLocked,
          action: onToggleUiZoomLock
        },
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
        },
        {
          label: t.lockTerminalZoom,
          checked: terminalZoomLocked,
          action: onToggleTerminalZoomLock
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
        if (!isDesktop || (event.target instanceof Element && event.target.closest('button'))) {
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
        {isDesktop ? (
          <button
            aria-expanded={openMenu?.kind === 'window'}
            aria-haspopup="menu"
            className={openMenu?.kind === 'window' ? 'is-open' : undefined}
            type="button"
            onClick={(event) => openMenuAt('window', event.currentTarget)}
          >
            {t.nativeMenuWindow}
          </button>
        ) : null}
      </div>
      {isDesktop ? (
        <div className="window-control-buttons">
          <button
            aria-label={t.windowMinimize}
            type="button"
            onClick={() => {
              void desktopApi?.minimizeCurrentWindow()
            }}
          >
            <AppIcon name="window-minimize" size={10} strokeWidth={1.5} />
          </button>
          <button
            aria-label={isMaximized ? t.windowRestore : t.windowMaximize}
            type="button"
            onClick={() => {
              void desktopApi?.toggleMaximizeCurrentWindow()
            }}
          >
            <AppIcon name={isMaximized ? 'window-restore' : 'window-maximize'} size={10} strokeWidth={1.5} />
          </button>
          <CloseButton
            aria-label={t.windowClose}
            onClick={() => {
              void desktopApi?.closeCurrentWindow()
            }}
            iconStrokeWidth={2.4}
            size="window"
          />
        </div>
      ) : null}
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
