import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { ErrorBoundary } from './features/common/ErrorBoundary'
import { createTauriApi } from '../bridge/tauri-api'
import { getCurrentWindow } from '@tauri-apps/api/window'
import './styles/index.css'

const initialWindowMode = new URLSearchParams(window.location.search).get('window') ?? 'main'
// Keep renderer-only chrome adjustments scoped to the Tauri window model.
// macOS Overlay uses a different native title-bar baseline than Electron's
// hiddenInset, even when both apps render the same React title bar.
document.documentElement.dataset.runtime = 'tauri'
document.documentElement.classList.toggle('tauri-standalone-window', initialWindowMode !== 'main')

// Global window dragging handler for Tauri custom titlebars. Capture the event
// before Monaco/child controls can stop propagation; the drag-region checks
// below still keep buttons, tabs, and editable content interactive.
const handleWindowMouseDown = (e: MouseEvent) => {
  // Only trigger dragging on left-click
  if (e.button !== 0) return

  let target = e.target as HTMLElement | null
  let isDragRegion = false

  while (target) {
    const tagName = target.tagName.toLowerCase()

    // Exclude basic interactive elements
    if (
      tagName === 'button' ||
      tagName === 'input' ||
      tagName === 'textarea' ||
      tagName === 'select' ||
      tagName === 'a'
    ) {
      return
    }

    // Exclude tabs, tab-tools, and other interactive containers
    if (
      target.getAttribute('role') === 'button' ||
      target.getAttribute('contenteditable') === 'true' ||
      target.classList.contains('fs-tab') ||
      target.classList.contains('add-tab') ||
      target.classList.contains('window-tools') ||
      target.classList.contains('window-controls-decorator') ||
      target.closest('.no-drag')
    ) {
      return
    }

    if (target.hasAttribute('data-tauri-drag-region')) {
      isDragRegion = true
      break
    }

    target = target.parentElement
  }

  if (isDragRegion) {
    void getCurrentWindow()
      .startDragging()
      .catch((err) => console.error('Failed to start window dragging:', err))
  }
}

window.addEventListener('mousedown', handleWindowMouseDown, true)

window.fileterm = createTauriApi()
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>
)
