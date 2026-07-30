const WINDOW_CHROME_PREVIEW_PLATFORM = 'linux'

/**
 * Keeps platform behavior sourced from the Tauri bridge while allowing the
 * Linux renderer chrome to be inspected on another desktop during `vite`
 * development. Vite replaces `import.meta.env.DEV` with `false` in production,
 * so release builds always use the actual runtime platform.
 */
export function resolveRendererPlatform(platform: string): string {
  if (import.meta.env.DEV && import.meta.env.VITE_WINDOW_CHROME_PREVIEW === WINDOW_CHROME_PREVIEW_PLATFORM) {
    return WINDOW_CHROME_PREVIEW_PLATFORM
  }

  return platform
}
