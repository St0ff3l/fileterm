import { getName, getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import type { LinuxWindowCornerStyle } from '@fileterm/core'

export async function getRuntimeMetadata() {
  const [nativePlatform, arch, runtimeVersion, appVersion, appName] = await Promise.all([
    invoke<string>('app_get_platform'),
    invoke<string>('app_get_arch'),
    invoke<string>('app_get_runtime_version'),
    getVersion(),
    getName()
  ])
  const linuxWindowCornerStyle =
    nativePlatform === 'linux'
      ? await invoke<LinuxWindowCornerStyle>('app_get_linux_window_corner_style').catch(() => 'square' as const)
      : undefined
  const platform =
    nativePlatform === 'macos' || nativePlatform === 'darwin'
      ? 'darwin'
      : nativePlatform === 'windows' || nativePlatform === 'win32'
        ? 'win32'
        : 'linux'

  return { platform, linuxWindowCornerStyle, arch, runtimeVersion, appVersion, appName }
}
