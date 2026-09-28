import { DEFAULT_UI_ZOOM_PERCENT, MAX_UI_ZOOM_PERCENT, MIN_UI_ZOOM_PERCENT } from '@fileterm/core'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'

export async function applyUiZoomPercent(value: number) {
  const percent = Number.isFinite(value)
    ? Math.max(MIN_UI_ZOOM_PERCENT, Math.min(MAX_UI_ZOOM_PERCENT, value))
    : DEFAULT_UI_ZOOM_PERCENT
  const factor = percent / 100
  const root = document.documentElement

  root.style.setProperty('--app-ui-zoom', String(factor))

  if (!isTauri()) {
    return
  }

  try {
    await getCurrentWebview().setZoom(factor)
  } catch (error) {
    root.style.setProperty('--app-ui-zoom', '1')
    console.warn('Failed to apply app interface zoom:', error)
  }
}

export function getUiZoomFactor() {
  if (typeof document === 'undefined') {
    return 1
  }

  const factor = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--app-ui-zoom').trim())
  return Number.isFinite(factor) && factor > 0 ? factor : 1
}

export function scaleTerminalFontSizeForUiZoom(fontSize: number) {
  return fontSize / getUiZoomFactor()
}
