/**
 * xterm and Monaco cache glyph dimensions in their canvas renderers. In a
 * packaged webview a local font can finish loading, or the window can move to
 * a display with a different scale factor, after either component is mounted.
 * A regular DOM layout corrects itself in that situation; canvas-backed text
 * does not. Keep the font stack local to the app bundle and provide one
 * observer that asks consumers to remeasure at both boundaries.
 */
import { DEFAULT_TERMINAL_LINE_HEIGHT } from '@fileterm/core'
import { getImportedFontWeightForBold } from './imported-fonts'
import { APP_EVENT, onAppEvent } from '../lib/app-events'
import { MONO_FONT_FALLBACK } from './font-stacks'

export function getTerminalFontWeight(): 400 | 500 | 600 {
  if (typeof document === 'undefined') return 400
  const weight = Number(getComputedStyle(document.documentElement).getPropertyValue('--terminal-font-weight'))
  return weight === 500 || weight === 600 ? weight : 400
}

export function setTerminalFontWeight(weight: number) {
  document.documentElement.style.setProperty('--terminal-font-weight', String(weight))
}

export function getTerminalFontBoldWeight() {
  if (typeof document === 'undefined') return 700
  const stack = getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim()
  const firstFamily = stack.match(/^(?:"([^"]+)"|'([^']+)'|([^,]+))/)
  const family = firstFamily?.[1] ?? firstFamily?.[2] ?? firstFamily?.[3]?.trim()
  if (!family) return 700
  return getImportedFontWeightForBold(family) ?? getTerminalFontWeight()
}

export function getTerminalLineHeight() {
  if (typeof document === 'undefined') return DEFAULT_TERMINAL_LINE_HEIGHT
  const lineHeight = Number(getComputedStyle(document.documentElement).getPropertyValue('--terminal-line-height'))
  return Number.isFinite(lineHeight) && lineHeight >= 1 && lineHeight <= 1.5 ? lineHeight : DEFAULT_TERMINAL_LINE_HEIGHT
}

export function setTerminalLineHeight(lineHeight: number) {
  document.documentElement.style.setProperty('--terminal-line-height', String(lineHeight))
}

export const FILETERM_MONO_FONT_FAMILY = MONO_FONT_FALLBACK

export function getConfiguredMonoFontFamily() {
  if (typeof document === 'undefined') return FILETERM_MONO_FONT_FAMILY
  const configured = getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim()
  return configured ? `${configured}, ${FILETERM_MONO_FONT_FAMILY}` : FILETERM_MONO_FONT_FAMILY
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
      onMetricsChanged(configuredFontFamily)
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
  const loadConfiguredFonts = () => {
    const family = getConfiguredMonoFontFamily()
    void Promise.all([
      document.fonts.load(`${getTerminalFontWeight()} 12px ${family}`),
      document.fonts.load(`600 13px ${family}`),
      document.fonts.load(`700 12px ${family}`)
    ])
      .catch(() => {
        // Keep bundled fallbacks usable when an imported face fails decoding.
      })
      .finally(() => {
        void document.fonts.ready.then(notify).catch(notify)
      })
  }
  const disposeImportedFontsListener = onAppEvent(APP_EVENT.importedFontsChanged, loadConfiguredFonts)
  // Theme changes and imported faces can start loading after the initial
  // fonts.ready promise has already resolved. Remeasure their final glyphs too.
  document.fonts.addEventListener('loadingdone', notify)
  document.fonts.addEventListener('loadingerror', notify)

  let observedFamily = getConfiguredMonoFontFamily()
  let observedWeight = getTerminalFontWeight()
  const fontObserver = new MutationObserver(() => {
    const nextFamily = getConfiguredMonoFontFamily()
    const nextWeight = getTerminalFontWeight()
    if (nextFamily !== observedFamily || nextWeight !== observedWeight) {
      observedFamily = nextFamily
      observedWeight = nextWeight
      notify()
      loadConfiguredFonts()
    }
  })
  fontObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['style', 'class', 'data-theme']
  })

  loadConfiguredFonts()

  return () => {
    disposed = true
    cancelPaint?.()
    pixelRatioQuery.removeEventListener('change', onPixelRatioChanged)
    window.removeEventListener('resize', onViewportResize)
    window.visualViewport?.removeEventListener('resize', onViewportResize)
    document.fonts.removeEventListener('loadingdone', notify)
    document.fonts.removeEventListener('loadingerror', notify)
    disposeImportedFontsListener()
    fontObserver.disconnect()
  }
}
