import type { LinuxWindowCornerStyle } from '@fileterm/core'

export interface BridgeMetadata {
  platform: string
  linuxWindowCornerStyle?: LinuxWindowCornerStyle
  arch: string
  appVersion: string
  appName: string
  runtimeName: 'Electron' | 'Tauri' | 'Web'
  runtimeVersion: string
  isDesktop: boolean
}

export interface BridgeTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>
  invokeWithChannel<T, E>(command: string, args: Record<string, unknown>, onEvent: (event: E) => void): Promise<T>
  subscribe<T>(eventName: string, listener: (payload: T) => void): () => void
  subscribeReady<T>(eventName: string, listener: (payload: T) => void): Promise<() => void>
}
