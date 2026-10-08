export const MONO_FONT_FALLBACK = '"JetBrains Mono", "Noto Sans SC", Menlo, Consolas, "Liberation Mono", monospace'
export const UI_FONT_FALLBACK = '"Inter", "Noto Sans SC", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'

export const SYSTEM_FONT_CANDIDATES = {
  'SF Mono': ['SF Mono', 'SFMono-Regular'],
  'SF Pro Text': ['SF Pro Text', 'SFProText-Regular']
} as const

/** Settings store a family name, including imported names containing commas.
 * Quote it as one CSS family and always retain local, bundled fallbacks. */
export function configuredFontStack(
  family: string,
  kind: 'code' | 'ui',
  platform = typeof window === 'undefined' ? undefined : window.fileterm?.platform
) {
  // Apple's protected UI fonts are exposed through generic system families,
  // not necessarily by their public marketing names or local() font faces.
  if (platform === 'darwin' && family === 'SF Mono') return `ui-monospace, ${MONO_FONT_FALLBACK}`
  if (platform === 'darwin' && family === 'SF Pro Text') return `system-ui, ${UI_FONT_FALLBACK}`
  const candidates = SYSTEM_FONT_CANDIDATES[family as keyof typeof SYSTEM_FONT_CANDIDATES] ?? [family]
  return `${candidates.map((name) => JSON.stringify(name)).join(', ')}, ${kind === 'code' ? MONO_FONT_FALLBACK : UI_FONT_FALLBACK}`
}

/** System generic font access is available on macOS without a local() face. */
export function systemFontAvailability(platform: string | undefined) {
  return { 'SF Mono': platform === 'darwin', 'SF Pro Text': platform === 'darwin' }
}
