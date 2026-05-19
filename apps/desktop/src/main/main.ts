import { app, BrowserWindow } from 'electron'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { registerIpcHandlers } from './ipc/index.js'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

let mainWindow: BrowserWindow | null = null
let connectionManagerWindow: BrowserWindow | null = null
let connectionFormWindow: BrowserWindow | null = null
let commandManagerWindow: BrowserWindow | null = null
let commandFormWindow: BrowserWindow | null = null
let fileEditorWindow: BrowserWindow | null = null

const isMac = process.platform === 'darwin'

function loadAppWindow(win: BrowserWindow, searchParams?: Record<string, string>) {
  if (!app.isPackaged) {
    const url = new URL('http://localhost:5188')
    Object.entries(searchParams ?? {}).forEach(([key, value]) => {
      url.searchParams.set(key, value)
    })
    win.loadURL(url.toString())
    return
  }

  win.loadFile(path.join(__dirname, '../../dist/index.html'), {
    query: searchParams
  })
}

function createMainWindow() {
  const win = new BrowserWindow({
    width: 1360,
    height: 900,
    minWidth: 1040,
    minHeight: 680,
    title: 'TermDock',
    titleBarStyle: isMac ? 'hiddenInset' : 'default',
    trafficLightPosition: isMac ? { x: 20, y: 18 } : undefined,
    backgroundColor: '#151515',
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  })

  mainWindow = win
  win.on('closed', () => {
    if (mainWindow === win) {
      mainWindow = null
    }
  })

  if (!app.isPackaged) {
    loadAppWindow(win)
    win.webContents.on('did-finish-load', async () => {
      try {
        const hasDesktopApi = await win.webContents.executeJavaScript('Boolean(window.termdock?.isDesktop)')
        console.log(`[TermDock] preload ready: ${hasDesktopApi}`)
      } catch (error) {
        console.error('[TermDock] preload probe failed', error)
      }
    })
    win.webContents.openDevTools({ mode: 'detach' })
    return win
  }

  loadAppWindow(win)
  return win
}

function createNativeChildWindow(options: {
  title: string
  width: number
  height: number
  minWidth: number
  minHeight: number
  backgroundColor?: string
}) {
  return new BrowserWindow({
    width: options.width,
    height: options.height,
    minWidth: options.minWidth,
    minHeight: options.minHeight,
    show: false,
    title: options.title,
    backgroundColor: options.backgroundColor ?? '#1b1b1b',
    autoHideMenuBar: true,
    titleBarStyle: isMac ? 'hiddenInset' : 'default',
    trafficLightPosition: isMac ? { x: 16, y: 14 } : undefined,
    minimizable: false,
    vibrancy: isMac ? 'sidebar' : undefined,
    visualEffectState: isMac ? 'active' : undefined,
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  })
}

function openConnectionManagerWindow(parent: BrowserWindow) {
  void parent
  if (connectionManagerWindow && !connectionManagerWindow.isDestroyed()) {
    connectionManagerWindow.focus()
    return
  }

  const win = createNativeChildWindow({
    title: '连接管理器',
    width: 980,
    height: 680,
    minWidth: 760,
    minHeight: 520
  })

  connectionManagerWindow = win
  win.once('ready-to-show', () => {
    win.show()
  })
  win.on('closed', () => {
    if (connectionManagerWindow === win) {
      connectionManagerWindow = null
    }
  })

  loadAppWindow(win, { window: 'connection-manager' })
}

function openCommandManagerWindow(parent: BrowserWindow) {
  void parent
  if (commandManagerWindow && !commandManagerWindow.isDestroyed()) {
    commandManagerWindow.focus()
    return
  }

  const win = createNativeChildWindow({
    title: '命令管理器',
    width: 1180,
    height: 760,
    minWidth: 980,
    minHeight: 640
  })

  commandManagerWindow = win
  win.once('ready-to-show', () => {
    win.show()
  })
  win.on('closed', () => {
    if (commandManagerWindow === win) {
      commandManagerWindow = null
    }
  })

  loadAppWindow(win, { window: 'command-manager' })
}

function openConnectionFormWindow(parent: BrowserWindow, mode: 'create' | 'edit', profileId?: string) {
  void parent
  if (connectionFormWindow && !connectionFormWindow.isDestroyed()) {
    connectionFormWindow.close()
  }

  const win = createNativeChildWindow({
    title: mode === 'edit' ? '编辑连接' : '新建连接',
    width: 920,
    height: 720,
    minWidth: 760,
    minHeight: 620
  })

  connectionFormWindow = win
  win.once('ready-to-show', () => {
    win.show()
  })
  win.on('closed', () => {
    if (connectionFormWindow === win) {
      connectionFormWindow = null
    }
  })

  loadAppWindow(win, {
    window: 'connection-form',
    mode,
    ...(profileId ? { profileId } : {})
  })
}

function openCommandFormWindow(parent: BrowserWindow, mode: 'create' | 'edit', commandId?: string, folderId?: string) {
  void parent
  if (commandFormWindow && !commandFormWindow.isDestroyed()) {
    commandFormWindow.close()
  }

  const win = createNativeChildWindow({
    title: mode === 'edit' ? '编辑命令' : '新建命令',
    width: 960,
    height: 760,
    minWidth: 760,
    minHeight: 620
  })

  commandFormWindow = win
  win.once('ready-to-show', () => {
    win.show()
  })
  win.on('closed', () => {
    if (commandFormWindow === win) {
      commandFormWindow = null
    }
  })

  loadAppWindow(win, {
    window: 'command-form',
    mode,
    ...(commandId ? { commandId } : {}),
    ...(folderId ? { folderId } : {})
  })
}

function openFileEditorWindow(parent: BrowserWindow, input: {
  source: 'local' | 'remote'
  path: string
  name: string
  tabId?: string
  encoding?: string
}) {
  void parent
  if (fileEditorWindow && !fileEditorWindow.isDestroyed()) {
    fileEditorWindow.close()
  }

  const win = createNativeChildWindow({
    title: `编辑文件 - ${input.name}`,
    width: 1120,
    height: 460,
    minWidth: 920,
    minHeight: 400,
    backgroundColor: '#171b20'
  })

  fileEditorWindow = win
  win.once('ready-to-show', () => {
    win.center()
    win.show()
  })
  win.on('closed', () => {
    if (fileEditorWindow === win) {
      fileEditorWindow = null
    }
  })

  loadAppWindow(win, {
    window: 'file-editor',
    source: input.source,
    path: input.path,
    name: input.name,
    ...(input.tabId ? { tabId: input.tabId } : {}),
    ...(input.encoding ? { encoding: input.encoding } : {})
  })
}

app.whenReady().then(() => {
  registerIpcHandlers(app.getPath('userData'), {
    getMainWindow: () => mainWindow,
    openConnectionManagerWindow,
    openCommandManagerWindow,
    openConnectionFormWindow,
    openCommandFormWindow,
    openFileEditorWindow
  })
  createMainWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
