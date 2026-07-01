/// <reference types="vite/client" />
import type { FileTermDesktopApi } from '@fileterm/core'

declare global {
  interface Window {
    fileterm?: FileTermDesktopApi
  }
}

export {}
