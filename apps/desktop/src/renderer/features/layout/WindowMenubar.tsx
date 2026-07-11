import type { FileTermDesktopApi } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'

export function WindowMenubar({ desktopApi, isMaximized }: { desktopApi?: FileTermDesktopApi; isMaximized: boolean }) {
  const openWindowMenu = (menu: 'file' | 'view' | 'window', target: HTMLButtonElement) => {
    const rect = target.getBoundingClientRect()
    void desktopApi?.showWindowMenu(menu, Math.round(rect.left), Math.round(rect.bottom))
  }

  return (
    <div className="window-menubar">
      <div className="window-menu-items">
        <button type="button" onClick={(event) => openWindowMenu('file', event.currentTarget)}>
          File
        </button>
        <button type="button" onClick={(event) => openWindowMenu('view', event.currentTarget)}>
          View
        </button>
        <button type="button" onClick={(event) => openWindowMenu('window', event.currentTarget)}>
          Window
        </button>
      </div>
      <div className="window-control-buttons">
        <button
          aria-label="Minimize"
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
          aria-label="Maximize"
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
          aria-label="Close"
          onClick={() => {
            void desktopApi?.closeCurrentWindow()
          }}
          size="window"
        />
      </div>
    </div>
  )
}
