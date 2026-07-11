import { useEffect, useState, type ReactNode } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'

export function StandaloneWindowFrame({
  children,
  isWindows,
  showPlatformTitlebar = true,
  title
}: {
  children: ReactNode
  isWindows: boolean
  showPlatformTitlebar?: boolean
  title: string
}) {
  const shouldShowPlatformTitlebar = isWindows && showPlatformTitlebar
  return (
    <div className={`standalone-window-frame ${shouldShowPlatformTitlebar ? 'has-standalone-titlebar' : ''}`}>
      <StandaloneWindowTitlebar isWindows={shouldShowPlatformTitlebar} title={title} />
      {children}
    </div>
  )
}

export function StandaloneWindowTitlebar({ isWindows, title }: { isWindows: boolean; title: string }) {
  const desktopApi = window.fileterm
  const [isMaximized, setIsMaximized] = useState(false)

  useEffect(() => {
    if (!isWindows || !desktopApi) {
      return
    }
    desktopApi.isCurrentWindowMaximized().then(setIsMaximized).catch(console.error)
    const unsubscribe = desktopApi.onWindowMaximizedChange(setIsMaximized)
    return unsubscribe
  }, [isWindows, desktopApi])

  if (!isWindows) {
    return null
  }

  return (
    <div className="standalone-window-titlebar">
      <div className="window-brandmark">
        <AppIcon name="brand" size={18} />
        <strong>{t.appTitle}</strong>
        <span>{title}</span>
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
