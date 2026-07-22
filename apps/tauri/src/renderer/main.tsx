import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { ErrorBoundary } from './features/common/ErrorBoundary'
import { createTauriApi } from '../bridge/tauri-api'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { t } from './i18n'
import './styles/index.css'

const initialWindowMode = new URLSearchParams(window.location.search).get('window') ?? 'main'
// Keep renderer-only chrome adjustments scoped to the Tauri window model.
// macOS Overlay uses a different native title-bar baseline than Electron's
// hiddenInset, even when both apps render the same React title bar.
document.documentElement.dataset.runtime = 'tauri'
document.documentElement.classList.toggle('tauri-standalone-window', initialWindowMode !== 'main')

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

// Start a native drag only from an explicitly marked, non-interactive area.
// The old ancestor walk treated large standalone containers as deep drag
// regions on Windows and could consume clicks meant for form controls.
const handleWindowMouseDown = (e: MouseEvent) => {
  if (e.button !== 0) return
  const target = e.target instanceof Element ? e.target : null
  if (!target || target.closest(interactiveWindowSelector)) return
  if (!target.closest('[data-tauri-drag-region]')) return

  void getCurrentWindow()
    .startDragging()
    .catch((err) => console.error('Failed to start window dragging:', err))
}

window.addEventListener('mousedown', handleWindowMouseDown, true)

const root = ReactDOM.createRoot(document.getElementById('root')!)

void createTauriApi()
  .then((api) => {
    // Runtime metadata is synchronous in the shared desktop contract. Mount
    // only after native metadata resolves so first-read consumers never see
    // placeholder version, architecture, or platform fields.
    window.fileterm = api
    document.documentElement.dataset.platform = api.platform
    root.render(
      <React.StrictMode>
        <ErrorBoundary>
          <App />
        </ErrorBoundary>
      </React.StrictMode>
    )
  })
  .catch((error: unknown) => {
    console.error('Failed to initialize the Tauri desktop bridge:', error)
    root.render(
      <div role="alert" className="app-bootstrap-error">
        {t.runtimeInitFailed}
      </div>
    )
  })
