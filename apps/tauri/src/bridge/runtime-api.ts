import type { FileTermDesktopApi } from '@fileterm/core'
import { isTauri } from '@tauri-apps/api/core'

export function isDesktopRuntime(): boolean {
  return isTauri()
}

export async function createRuntimeApi(): Promise<FileTermDesktopApi> {
  if (isDesktopRuntime()) {
    const { createTauriApi } = await import('./tauri-api')
    return createTauriApi()
  }
  const { createWebApi } = await import('./web-api')
  return createWebApi()
}

export async function showCurrentWindowIfDesktop(): Promise<void> {
  if (!isDesktopRuntime()) {
    return
  }
  const { showCurrentWindow } = await import('./tauri-api')
  return showCurrentWindow()
}
