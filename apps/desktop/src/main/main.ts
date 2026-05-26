import { app, BrowserWindow } from 'electron'
import fs from 'node:fs'
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
const DEFAULT_UI_PREFERENCES = {
  theme: 'default-dark',
  locale: 'zhCN'
} as const

type UiPreferences = {
  theme: 'default-dark' | 'default-light'
  locale: 'zhCN' | 'enUS'
}

let uiPreferences: UiPreferences = { ...DEFAULT_UI_PREFERENCES }
let ipcServices: ReturnType<typeof registerIpcHandlers> | null = null

function isBrokenPipeError(error: unknown): boolean {
  return error instanceof Error
    && 'code' in error
    && (error as NodeJS.ErrnoException).code === 'EPIPE'
}

function safeConsoleError(...args: unknown[]) {
  try {
    console.error(...args)
  } catch (error) {
    if (!isBrokenPipeError(error)) {
      throw error
    }
  }
}

process.stdout.on('error', (error) => {
  if (!isBrokenPipeError(error)) {
    throw error
  }
})

process.stderr.on('error', (error) => {
  if (!isBrokenPipeError(error)) {
    throw error
  }
})

process.on('uncaughtException', (error) => {
  if (isBrokenPipeError(error)) {
    return
  }

  safeConsoleError('[TermDock] uncaught exception', error)
})

function getUiPreferencesPath() {
  return path.join(app.getPath('userData'), 'ui-preferences.json')
}

function normalizeUiPreferences(input?: Partial<UiPreferences> | null): UiPreferences {
  return {
    theme: input?.theme === 'default-light' ? 'default-light' : 'default-dark',
    locale: input?.locale === 'enUS' ? 'enUS' : 'zhCN'
  }
}

function readUiPreferences() {
  try {
    const raw = fs.readFileSync(getUiPreferencesPath(), 'utf-8')
    uiPreferences = normalizeUiPreferences(JSON.parse(raw) as Partial<UiPreferences>)
  } catch {
    uiPreferences = { ...DEFAULT_UI_PREFERENCES }
  }
}

function writeUiPreferences(next: UiPreferences) {
  uiPreferences = next
  try {
    fs.writeFileSync(getUiPreferencesPath(), JSON.stringify(next, null, 2), 'utf-8')
  } catch (error) {
    safeConsoleError('[TermDock] failed to persist ui preferences', error)
  }
}

function updateUiPreferences(input: Partial<UiPreferences>) {
  const next = normalizeUiPreferences({
    ...uiPreferences,
    ...input
  })
  writeUiPreferences(next)
  return next
}

function getWindowBackgroundColor(theme: UiPreferences['theme']) {
  return theme === 'default-light' ? '#f8fafc' : '#151515'
}

function getAppIconPath() {
  return [
    path.join(__dirname, '../../build/icon.png'),
    path.join(__dirname, '../../public/icon.png'),
    path.join(__dirname, '../../dist/icon.png')
  ].find((candidate) => fs.existsSync(candidate))
}

function getWindowIconOptions() {
  const icon = getAppIconPath()
  return icon ? { icon } : {}
}

function loadAppWindow(win: BrowserWindow, searchParams?: Record<string, string>, preferences: UiPreferences = uiPreferences) {
  const query = {
    ...(searchParams ?? {}),
    theme: preferences.theme,
    locale: preferences.locale
  }

  if (!app.isPackaged) {
    const url = new URL('http://localhost:5188')
    Object.entries(query).forEach(([key, value]) => {
      url.searchParams.set(key, value)
    })
    win.loadURL(url.toString())
    return
  }

  win.loadFile(path.join(__dirname, '../../dist/index.html'), {
    query
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
    backgroundColor: getWindowBackgroundColor(uiPreferences.theme),
    ...getWindowIconOptions(),
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
    loadAppWindow(win, undefined, uiPreferences)
    win.webContents.on('did-finish-load', async () => {
      try {
        const hasDesktopApi = await win.webContents.executeJavaScript('Boolean(window.termdock?.isDesktop)')
        console.log(`[TermDock] preload ready: ${hasDesktopApi}`)
      } catch (error) {
        safeConsoleError('[TermDock] preload probe failed', error)
      }
    })
    win.webContents.openDevTools({ mode: 'detach' })
    return win
  }

  loadAppWindow(win, undefined, uiPreferences)
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
    backgroundColor: options.backgroundColor ?? getWindowBackgroundColor(uiPreferences.theme),
    autoHideMenuBar: true,
    titleBarStyle: isMac ? 'hiddenInset' : 'default',
    trafficLightPosition: isMac ? { x: 16, y: 14 } : undefined,
    minimizable: false,
    vibrancy: isMac ? 'sidebar' : undefined,
    visualEffectState: isMac ? 'active' : undefined,
    ...getWindowIconOptions(),
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  })
}

function centerChildWindowToParent(parent: BrowserWindow | null, child: BrowserWindow) {
  if (!parent || parent.isDestroyed()) {
    child.center()
    return
  }

  const parentBounds = parent.getBounds()
  const childBounds = child.getBounds()
  const x = Math.round(parentBounds.x + (parentBounds.width - childBounds.width) / 2)
  const y = Math.round(parentBounds.y + (parentBounds.height - childBounds.height) / 2)
  child.setPosition(x, y)
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

  loadAppWindow(win, { window: 'connection-manager' }, uiPreferences)
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

  loadAppWindow(win, { window: 'command-manager' }, uiPreferences)
}

function openConnectionFormWindow(parent: BrowserWindow, mode: 'create' | 'edit', profileId?: string) {
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
    centerChildWindowToParent(parent, win)
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
  }, uiPreferences)
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
  }, uiPreferences)
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
    width: 1600,
    height: 1040,
    minWidth: 1040,
    minHeight: 640,
    backgroundColor: uiPreferences.theme === 'default-light' ? '#f8fafc' : '#171b20'
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
  }, uiPreferences)
}

app.whenReady().then(() => {
  readUiPreferences()
  const appIconPath = getAppIconPath()
  if (isMac && appIconPath) {
    app.dock?.setIcon(appIconPath)
  }
  ipcServices = registerIpcHandlers(app.getPath('userData'), {
    getMainWindow: () => mainWindow,
    getUiPreferences: () => uiPreferences,
    setUiPreferences: (input) => {
      const next = updateUiPreferences(input)
      for (const window of BrowserWindow.getAllWindows()) {
        if (!window.isDestroyed()) {
          window.setBackgroundColor(getWindowBackgroundColor(next.theme))
        }
      }
      return next
    },
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
  void ipcServices?.workspaceService.shutdown()
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('before-quit', () => {
  void ipcServices?.workspaceService.shutdown()
})
