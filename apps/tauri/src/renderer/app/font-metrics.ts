/**
 * xterm and Monaco cache glyph dimensions in their canvas renderers. In a
 * packaged webview a local font can finish loading, or the window can move to
 * a display with a different scale factor, after either component is mounted.
 * A regular DOM layout corrects itself in that situation; canvas-backed text
 * does not. Keep the font stack local to the app bundle and provide one
 * observer that asks consumers to remeasure at both boundaries.
 */
import { APP_EVENT, onAppEvent } from '../lib/app-events'

export const FILETERM_MONO_FONT_FAMILY = '"JetBrains Mono", "SF Mono", Menlo, Consolas, "Liberation Mono", monospace'

export function getConfiguredMonoFontFamily() {
  if (typeof document === 'undefined') return FILETERM_MONO_FONT_FAMILY
  const configured = getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim()
  return configured || FILETERM_MONO_FONT_FAMILY
}

function nextPaint(callback: () => void) {
  let frame = window.requestAnimationFrame(() => {
    frame = window.requestAnimationFrame(callback)
  })
  return () => window.cancelAnimationFrame(frame)
}

/**
 * Runs after bundled mono fonts become available and whenever WebView changes
 * device pixel ratio. The returned disposer prevents a closed standalone
 * editor from receiving a late font event.
 */
export function observeCanvasTextMetrics(onMetricsChanged: (fontFamily: string) => void) {
  let disposed = false
  let cancelPaint: (() => void) | null = null
  let pixelRatio = window.devicePixelRatio || 1
  let pixelRatioQuery = window.matchMedia(`(resolution: ${pixelRatio}dppx)`)

  const notify = () => {
    if (disposed) {
      return
    }
    cancelPaint?.()
    cancelPaint = nextPaint(() => {
      cancelPaint = null
      if (disposed) {
        return
      }
      const configuredFontFamily = getConfiguredMonoFontFamily()
      // xterm ignores an unchanged fontFamily, and its DOM renderer has no
      // texture atlas to clear. Pulse an equivalent stack to invalidate both
      // glyph widths and cell metrics, then restore the configured value.
      // Repeat the whole list: splitting on commas would break quoted names.
      onMetricsChanged(`${configuredFontFamily}, ${configuredFontFamily}`)
      if (!disposed) onMetricsChanged(configuredFontFamily)
    })
  }

  const rebindPixelRatioQuery = () => {
    pixelRatioQuery.removeEventListener('change', onPixelRatioChanged)
    pixelRatio = window.devicePixelRatio || 1
    pixelRatioQuery = window.matchMedia(`(resolution: ${pixelRatio}dppx)`)
    pixelRatioQuery.addEventListener('change', onPixelRatioChanged)
  }

  const onPixelRatioChanged = () => {
    rebindPixelRatioQuery()
    notify()
  }

  const onViewportResize = () => {
    const nextPixelRatio = window.devicePixelRatio || 1
    if (Math.abs(nextPixelRatio - pixelRatio) > 0.001) {
      rebindPixelRatioQuery()
      notify()
    }
  }

  pixelRatioQuery.addEventListener('change', onPixelRatioChanged)
  window.addEventListener('resize', onViewportResize)
  window.visualViewport?.addEventListener('resize', onViewportResize)
  const disposeImportedFontsListener = onAppEvent(APP_EVENT.importedFontsChanged, notify)
  // Theme changes and imported faces can start loading after the initial
  // fonts.ready promise has already resolved. Remeasure their final glyphs too.
  document.fonts.addEventListener('loadingdone', notify)
  document.fonts.addEventListener('loadingerror', notify)

  const configuredFontFamily = getConfiguredMonoFontFamily()
  void Promise.all([
    document.fonts.load(`400 12px ${configuredFontFamily}`),
    document.fonts.load(`600 13px ${configuredFontFamily}`)
  ])
    .catch(() => {
      // A system fallback remains usable if a local font cannot be decoded.
    })
    .finally(() => {
      void document.fonts.ready.then(notify).catch(notify)
    })

  return () => {
    disposed = true
    cancelPaint?.()
    pixelRatioQuery.removeEventListener('change', onPixelRatioChanged)
    window.removeEventListener('resize', onViewportResize)
    window.visualViewport?.removeEventListener('resize', onViewportResize)
    document.fonts.removeEventListener('loadingdone', notify)
    document.fonts.removeEventListener('loadingerror', notify)
    disposeImportedFontsListener()
  }
}
