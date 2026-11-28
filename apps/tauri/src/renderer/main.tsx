import React from 'react'
import ReactDOM from 'react-dom/client'
import { DEFAULT_TERMINAL_LINE_HEIGHT, DEFAULT_UI_FONT_SCALE_PERCENT, DEFAULT_UI_ZOOM_PERCENT } from '@fileterm/core'
import { App } from './app'
import { ErrorBoundary } from './features/common/error-boundary'
import { createRuntimeApi, isDesktopRuntime } from '../bridge/runtime-api'
import { getWebLoginStatus } from '../bridge/web-login'
import { WebLoginSettingsPanel } from './features/settings/settings-modal/panels/web-login-settings-panel'
import { getLocale, setLocale, t } from './i18n'
import { resolveRendererPlatform } from './lib/renderer-platform'
import { applyThemeVariables, type ThemeMode } from './app/theme-config'
import { applyUiZoomPercent } from './app/ui-zoom'
import { applyUiFontScalePercent } from './app/ui-font-scale'
import { setTerminalFontWeight, setTerminalLineHeight } from './app/font-metrics'
import { setTerminalRendererPreference } from './app/terminal-renderer-preference'
import { setTerminalZoomLockedPreference } from './app/terminal-zoom-lock-preference'
import { MainWindowReveal } from './lib/main-window-reveal'
import './styles/index.css'
import { installRendererErrorLogging, reportRendererError } from './lib/renderer-error-log'
import { installRendererPerformanceLogging } from './lib/renderer-performance-log'

const removeErrorLogging = installRendererErrorLogging()
if (import.meta.hot) import.meta.hot.dispose(removeErrorLogging)
const removePerformanceLogging = installRendererPerformanceLogging()
if (import.meta.hot) import.meta.hot.dispose(removePerformanceLogging)

const initialWindowMode = new URLSearchParams(window.location.search).get('window') ?? 'main'
// Keep renderer-only chrome adjustments scoped to the Tauri window model.
// macOS Overlay uses a different native title-bar baseline than Electron's
// hiddenInset, even when both apps render the same React title bar.
const isDesktop = isDesktopRuntime()
document.documentElement.dataset.runtime = isDesktop ? 'tauri' : 'web'
document.documentElement.classList.toggle('tauri-standalone-window', isDesktop && initialWindowMode !== 'main')

const syncWindowFocusState = (focused: boolean) => {
  document.documentElement.dataset.windowFocused = focused ? 'true' : 'false'
}

// DOM focus is not the same as native NSWindow focus. Keep the native state on
// <html> so platform theme skins can turn off glass when the window is idle.
document.documentElement.dataset.windowFocused = 'true'

const interactiveWindowSelector = [
  'button',
  'input',
  'textarea',
  'select',
  'a',
  '[role="button"]',
  '[role="menuitem"]',
  '[contenteditable="true"]',
  '[data-no-drag]',
  '.no-drag',
  '.fs-tab',
  '.add-tab',
  '.window-tools',
  '.window-controls-decorator'
].join(',')

if (isDesktop) {
  void import('@tauri-apps/api/window')
    .then(({ getCurrentWindow }) => {
      const currentWindow = getCurrentWindow()
      void currentWindow
        .isFocused()
        .then(syncWindowFocusState)
        .catch(() => undefined)
      void currentWindow.onFocusChanged(({ payload }) => syncWindowFocusState(payload)).catch(() => undefined)

      // Start a native drag only from an explicitly marked, non-interactive area.
      const handleWindowMouseDown = (e: MouseEvent) => {
        if (e.button !== 0) return
        const target = e.target instanceof Element ? e.target : null
        if (!target || target.closest(interactiveWindowSelector)) return
        if (!target.closest('[data-tauri-drag-region]')) return

        void currentWindow.startDragging().catch((err) => console.error('Failed to start window dragging:', err))
      }

      window.addEventListener('mousedown', handleWindowMouseDown, true)
    })
    .catch(() => undefined)
} else {
  window.addEventListener('focus', () => syncWindowFocusState(true))
  window.addEventListener('blur', () => syncWindowFocusState(false))
}

const root = ReactDOM.createRoot(document.getElementById('root')!)
const renderRoot = (content: React.ReactNode) => {
  root.render(
    <React.StrictMode>
      {isDesktop && initialWindowMode === 'main' && <MainWindowReveal />}
      {content}
    </React.StrictMode>
  )
}

void createRuntimeApi()
  .then((api) => {
    // Runtime metadata is synchronous in the shared desktop contract. Mount
    // only after native metadata resolves so first-read consumers never see
    // placeholder version, architecture, or platform fields.
    window.fileterm = api
    document.documentElement.dataset.platform = resolveRendererPlatform(api.platform)
    if (api.platform === 'linux') {
      document.documentElement.dataset.linuxWindowCorners = api.linuxWindowCornerStyle ?? 'square'
    }

    const webLogin = getWebLoginStatus()
    if (webLogin?.mode === 'login' || webLogin?.mode === 'setup') {
      applyThemeVariables('fileterm-dark')
      renderRoot(
        <main className="web-login-page">
          <div className="modal-card manager-modal settings-modal">
            <div className="connection-manager-header">
              <h1 className="connection-manager-title">{webLogin.mode === 'setup' ? t.settings : 'FileTerm'}</h1>
            </div>
            <WebLoginSettingsPanel status={webLogin} onSaved={() => window.location.reload()} />
          </div>
        </main>
      )
      return
    }

    // Read durable UI preferences before mounting React. App state is then
    // initialized from the saved values instead of briefly using defaults and
    // writing those defaults back over the persisted file.
    void api
      .getUiPreferences()
      .catch((error: unknown) => {
        console.warn('Failed to load UI preferences:', error)
        return undefined
      })
      .then(async (initialUiPreferences) => {
        setTerminalRendererPreference(initialUiPreferences?.terminalRenderer ?? 'webgl')
        setTerminalZoomLockedPreference(initialUiPreferences?.terminalZoomLocked ?? false)
        setTerminalFontWeight(initialUiPreferences?.terminalFontWeight ?? 400)
        setTerminalLineHeight(initialUiPreferences?.terminalLineHeight ?? DEFAULT_TERMINAL_LINE_HEIGHT)
        const searchParams = new URLSearchParams(window.location.search)
        const queryTheme = searchParams.get('theme')
        const isThemeMode = (val: unknown): val is ThemeMode =>
          val === 'fileterm-dark' ||
          val === 'fileterm-light' ||
          val === 'codex-dark' ||
          val === 'codex-light' ||
          val === 'default-dark' ||
          val === 'default-light'

        const initialTheme: ThemeMode = isThemeMode(queryTheme)
          ? queryTheme
          : initialUiPreferences && isThemeMode(initialUiPreferences.theme)
            ? initialUiPreferences.theme
            : 'fileterm-dark'
        const queryLocale = searchParams.get('locale')
        const initialLocale =
          queryLocale === 'enUS' || queryLocale === 'zhCN'
            ? queryLocale
            : initialUiPreferences?.locale === 'enUS' || initialUiPreferences?.locale === 'zhCN'
              ? initialUiPreferences.locale
              : getLocale()

        // Apply the loaded values before the first React render so CSS and the
        // proxy-backed translation table agree with App's initial state.
        applyThemeVariables(initialTheme, initialUiPreferences?.themeConfig)
        const initialUiZoomPercent = initialUiPreferences?.uiZoomPercent ?? DEFAULT_UI_ZOOM_PERCENT
        await applyUiZoomPercent(initialUiZoomPercent)
        applyUiFontScalePercent(
          initialUiPreferences?.uiFontScalePercent ?? DEFAULT_UI_FONT_SCALE_PERCENT,
          initialUiZoomPercent
        )
        setLocale(initialLocale)

        renderRoot(
          <ErrorBoundary>
            <App initialUiPreferences={initialUiPreferences} />
          </ErrorBoundary>
        )
      })
      .catch((error: unknown) => {
        reportRendererError('bootstrap', error)
        console.error('Failed to initialize the renderer:', error)
        renderRoot(
          <div role="alert" className="app-bootstrap-error">
            {t.runtimeInitFailed}
          </div>
        )
      })
  })
  .catch((error: unknown) => {
    console.error('Failed to initialize the Tauri desktop bridge:', error)
    renderRoot(
      <div role="alert" className="app-bootstrap-error">
        {t.runtimeInitFailed}
      </div>
    )
  })
