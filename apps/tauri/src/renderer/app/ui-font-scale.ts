import {
  DEFAULT_UI_FONT_SCALE_PERCENT,
  DEFAULT_UI_ZOOM_PERCENT,
  MAX_EFFECTIVE_UI_TEXT_SCALE_PERCENT,
  MAX_UI_FONT_SCALE_PERCENT,
  MAX_UI_ZOOM_PERCENT,
  MIN_UI_FONT_SCALE_PERCENT,
  MIN_UI_ZOOM_PERCENT
} from '@fileterm/core'

// shortcut: New fixed CSS font sizes need a matching entry here; centralize them if this list grows.
const UI_FONT_SIZES = [
  8, 9, 9.5, 10, 11, 11.5, 12, 12.5, 13, 13.5, 14, 15, 16, 17, 18, 19, 20, 21, 22, 24, 26, 28, 32, 34, 36
] as const

export function applyUiFontScalePercent(value: number, uiZoomPercent: number) {
  if (typeof document === 'undefined') {
    return
  }

  const fontPercent = Number.isFinite(value)
    ? Math.max(MIN_UI_FONT_SCALE_PERCENT, Math.min(MAX_UI_FONT_SCALE_PERCENT, value))
    : DEFAULT_UI_FONT_SCALE_PERCENT
  const zoomPercent = Number.isFinite(uiZoomPercent)
    ? Math.max(MIN_UI_ZOOM_PERCENT, Math.min(MAX_UI_ZOOM_PERCENT, uiZoomPercent))
    : DEFAULT_UI_ZOOM_PERCENT
  const percent = Math.min(fontPercent, (MAX_EFFECTIVE_UI_TEXT_SCALE_PERCENT * 100) / zoomPercent)
  const layoutScale = Math.max(1, percent / 100)
  const layoutGrowth = layoutScale - 1

  document.documentElement.style.setProperty('--app-ui-font-scale', String(percent / 100))
  document.documentElement.style.setProperty('--app-ui-layout-extra-xs', `${Number((layoutGrowth * 16).toFixed(2))}px`)
  document.documentElement.style.setProperty('--app-ui-layout-extra-sm', `${Number((layoutGrowth * 32).toFixed(2))}px`)
  document.documentElement.style.setProperty('--app-ui-layout-extra-md', `${Number((layoutGrowth * 56).toFixed(2))}px`)
  document.documentElement.style.setProperty('--app-ui-layout-extra-lg', `${Number((layoutGrowth * 112).toFixed(2))}px`)

  for (const size of UI_FONT_SIZES) {
    const token = String(size).replace('.', '-')
    document.documentElement.style.setProperty(
      `--ui-font-size-${token}px`,
      `${Number(((size * percent) / 100).toFixed(2))}px`
    )
  }
}

export function getUiFontScaleRatio() {
  if (typeof document === 'undefined') return 1
  const ratio = Number.parseFloat(document.documentElement.style.getPropertyValue('--app-ui-font-scale'))
  return Number.isFinite(ratio) ? Math.max(1, ratio) : 1
}
