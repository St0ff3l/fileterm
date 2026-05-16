import { app, BrowserWindow } from 'electron'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { registerIpcHandlers } from './ipc.js'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

function createWindow() {
  const win = new BrowserWindow({
    width: 1360,
    height: 900,
    minWidth: 1040,
    minHeight: 680,
    title: 'TermDock',
    titleBarStyle: 'hiddenInset',
    trafficLightPosition: { x: 20, y: 18 },
    backgroundColor: '#151515',
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  })

  if (!app.isPackaged) {
    win.loadURL('http://localhost:5188')
    win.webContents.on('did-finish-load', async () => {
      try {
        const hasDesktopApi = await win.webContents.executeJavaScript('Boolean(window.termdock?.isDesktop)')
        console.log(`[TermDock] preload ready: ${hasDesktopApi}`)
      } catch (error) {
        console.error('[TermDock] preload probe failed', error)
      }
    })
    win.webContents.openDevTools({ mode: 'detach' })
    return
  }

  win.loadFile(path.join(__dirname, '../../dist/index.html'))
}

app.whenReady().then(() => {
  registerIpcHandlers(app.getPath('userData'))
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
