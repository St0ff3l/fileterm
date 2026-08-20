import type { ImportedFont } from '@fileterm/core'
import { APP_EVENT, dispatchAppEvent } from '../lib/app-events'

const IMPORTED_FONT_STYLE_ID = 'fileterm-imported-fonts'
const importedFontSources = new Map<string, { font: ImportedFont; dataUrl: string }>()

function cssFontFormat(format: ImportedFont['format']) {
  return format === 'otf' ? 'opentype' : 'truetype'
}

function syncImportedFontStyle() {
  if (typeof document === 'undefined') return

  let style = document.getElementById(IMPORTED_FONT_STYLE_ID)
  if (!(style instanceof HTMLStyleElement)) {
    style = document.createElement('style')
    style.id = IMPORTED_FONT_STYLE_ID
    document.head.appendChild(style)
  }

  style.textContent = [...importedFontSources.values()]
    .map(({ font, dataUrl }) =>
      [
        '@font-face {',
        '  font-family: ' + JSON.stringify(font.family) + ';',
        '  src: url(' + JSON.stringify(dataUrl) + ') format(' + JSON.stringify(cssFontFormat(font.format)) + ');',
        '  font-display: swap;',
        '}'
      ].join('\n')
    )
    .join('\n')
}

function notifyImportedFontsChanged() {
  if (typeof window !== 'undefined') {
    dispatchAppEvent(APP_EVENT.importedFontsChanged)
  }
}

export function registerImportedFont(font: ImportedFont, dataUrl: string) {
  importedFontSources.set(font.id, { font, dataUrl })
  syncImportedFontStyle()
  notifyImportedFontsChanged()
}

export function registerImportedFonts(entries: Array<{ font: ImportedFont; dataUrl: string }>) {
  importedFontSources.clear()
  for (const entry of entries) {
    importedFontSources.set(entry.font.id, entry)
  }
  syncImportedFontStyle()
  notifyImportedFontsChanged()
}

export function unregisterImportedFont(fontId: string) {
  importedFontSources.delete(fontId)
  syncImportedFontStyle()
  notifyImportedFontsChanged()
}

export function clearImportedFonts() {
  importedFontSources.clear()
  if (typeof document !== 'undefined') {
    document.getElementById(IMPORTED_FONT_STYLE_ID)?.remove()
  }
}
