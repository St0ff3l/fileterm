import { app, BrowserWindow } from 'electron'
import electronUpdater from 'electron-updater'
import type { AppUpdateStatus } from '@fileterm/core'
import { appError, appLog } from './app-logger.js'

const { autoUpdater } = electronUpdater
// Keep macOS updates on the release page until Developer ID signing is available.
const MAC_AUTO_UPDATE_ENABLED = false
const RELEASE_BASE_URL = 'https://github.com/St0ff3l/fileterm/releases/tag'

export class AppUpdateService {
  private status: AppUpdateStatus
  private initialized = false
  private installing = false
  private checkPromise: Promise<AppUpdateStatus> | undefined

  constructor(private readonly onInstallRequested: () => void) {
    const isDesktopPlatform = process.platform === 'darwin' || process.platform === 'win32'
    const localDevUpdatesEnabled = !app.isPackaged && isDesktopPlatform
    const supported = (app.isPackaged || localDevUpdatesEnabled) && isDesktopPlatform
    if (localDevUpdatesEnabled) {
      autoUpdater.forceDevUpdateConfig = true
    }
    this.status = {
      state: supported ? 'idle' : 'unsupported',
      currentVersion: app.getVersion(),
      updateMode: process.platform === 'darwin' && !MAC_AUTO_UPDATE_ENABLED ? 'release-page' : 'in-app',
      ...(supported ? {} : { message: 'Updates are available only in packaged Windows and macOS builds.' })
    }
  }

  getStatus() {
    return this.status
  }

  isInstallingUpdate() {
    return this.installing
  }

  async checkForUpdates() {
    if (this.status.state === 'unsupported') {
      return this.status
    }
    if (this.checkPromise) {
      return this.checkPromise
    }

    this.initialize()
    this.checkPromise = (async () => {
      this.setStatus({ state: 'checking' })
      try {
        await autoUpdater.checkForUpdates()
      } catch (error) {
        this.setError(error)
      } finally {
        this.checkPromise = undefined
      }
      return this.status
    })()
    return this.checkPromise
  }

  async downloadUpdate() {
    if (this.status.state === 'unsupported') {
      return
    }

    this.initialize()
    try {
      this.setStatus({
        state: 'downloading',
        availableVersion: this.status.availableVersion,
        releaseUrl: this.status.releaseUrl,
        progress: 0
      })
      await autoUpdater.downloadUpdate()
    } catch (error) {
      this.setError(error)
    }
  }

  installUpdate() {
    if (this.status.state !== 'downloaded') {
      return
    }

    if (!app.isPackaged) {
      this.setStatus({
        state: 'error',
        availableVersion: this.status.availableVersion,
        releaseUrl: this.status.releaseUrl,
        progress: this.status.progress,
        message: '开发模式已完成下载；请使用已打包的 macOS 应用测试重启更新。'
      })
      return
    }

    this.onInstallRequested()
  }

  quitAndInstall() {
    if (!app.isPackaged) {
      return
    }
    this.installing = true
    autoUpdater.quitAndInstall(false, true)
  }

  private initialize() {
    if (this.initialized) {
      return
    }
    this.initialized = true

    autoUpdater.autoDownload = false
    autoUpdater.autoInstallOnAppQuit = false
    autoUpdater.on('checking-for-update', () => this.setStatus({ state: 'checking' }))
    autoUpdater.on('update-available', (info) => {
      this.setStatus({
        state: 'available',
        availableVersion: info.version,
        releaseUrl: `${RELEASE_BASE_URL}/v${info.version}`
      })
    })
    autoUpdater.on('update-not-available', () => this.setStatus({ state: 'not-available' }))
    autoUpdater.on('download-progress', (progress) => {
      this.setStatus({
        state: 'downloading',
        availableVersion: this.status.availableVersion,
        releaseUrl: this.status.releaseUrl,
        progress: Math.round(progress.percent)
      })
    })
    autoUpdater.on('update-downloaded', (info) => {
      this.setStatus({
        state: 'downloaded',
        availableVersion: info.version,
        releaseUrl: this.status.releaseUrl ?? `${RELEASE_BASE_URL}/v${info.version}`,
        progress: 100
      })
    })
    autoUpdater.on('error', (error) => this.setError(error))
  }

  private setError(error: unknown) {
    const message = error instanceof Error ? error.message : String(error)
    appError('[FileTerm] update check failed', error)
    this.setStatus({
      state: 'error',
      availableVersion: this.status.availableVersion,
      releaseUrl: this.status.releaseUrl,
      message
    })
  }

  private setStatus(next: Omit<AppUpdateStatus, 'currentVersion'>) {
    this.status = { currentVersion: app.getVersion(), updateMode: this.status.updateMode, ...next }
    appLog('[FileTerm] update status', this.status)
    for (const window of BrowserWindow.getAllWindows()) {
      if (!window.isDestroyed()) {
        window.webContents.send('app:update-status', this.status)
      }
    }
  }
}
