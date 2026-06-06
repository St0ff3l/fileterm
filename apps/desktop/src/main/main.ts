import { app, BrowserWindow, nativeTheme, Tray, Menu, nativeImage, shell } from 'electron'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { registerIpcHandlers } from './ipc/index.js'
import { appError, appLog, getAppLogDirectory, initAppLogger } from './services/app-logger.js'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

let mainWindow: BrowserWindow | null = null
let connectionManagerWindow: BrowserWindow | null = null
let connectionFormWindow: BrowserWindow | null = null
let commandManagerWindow: BrowserWindow | null = null
let commandFormWindow: BrowserWindow | null = null
let fileEditorWindow: BrowserWindow | null = null
let isQuitting = false
let tray: Tray | null = null

const isMac = process.platform === 'darwin'
const isWindows = process.platform === 'win32'
const DEFAULT_WINDOW_BOUNDS = {
  main: {
    width: 1280,
    height: 820,
    minWidth: 1040,
    minHeight: 680
  },
  connectionManager: {
    width: 860,
    height: 680,
    minWidth: 760,
    minHeight: 520
  },
  commandManager: {
    width: 860,
    height: 680,
    minWidth: 760,
    minHeight: 620
  },
  connectionForm: {
    width: 860,
    height: 680,
    minWidth: 760,
    minHeight: 620
  },
  commandForm: {
    width: 860,
    height: 680,
    minWidth: 760,
    minHeight: 620
  },
  fileEditor: {
    width: 1220,
    height: 780,
    minWidth: 1040,
    minHeight: 640
  }
} as const
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
    appError(...args)
  } catch (error) {
    if (!isBrokenPipeError(error)) {
      throw error
    }
  }
}

function attachWindowDiagnostics(win: BrowserWindow, label: string) {
  win.webContents.on('render-process-gone', (_event, details) => {
    safeConsoleError(`[TermDock] ${label} render-process-gone`, details)
  })
  win.webContents.on('unresponsive', () => {
    safeConsoleError(`[TermDock] ${label} became unresponsive`)
  })
  win.webContents.on('responsive', () => {
    safeConsoleError(`[TermDock] ${label} responsive again`)
  })
  win.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL, isMainFrame) => {
    safeConsoleError(`[TermDock] ${label} did-fail-load`, {
      errorCode,
      errorDescription,
      validatedURL,
      isMainFrame
    })
  })
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

async function openLogsDirectory() {
  const result = await shell.openPath(getAppLogDirectory())
  if (result) {
    throw new Error(result)
  }
}

function normalizeUiPreferences(input?: Partial<UiPreferences> | null): UiPreferences {
  return {
    theme: input?.theme === 'default-light' ? 'default-light' : 'default-dark',
    locale: input?.locale === 'enUS' ? 'enUS' : 'zhCN'
  }
}

function updateNativeThemeSource(theme: UiPreferences['theme']) {
  nativeTheme.themeSource = theme === 'default-light' ? 'light' : 'dark'
}

function readUiPreferences() {
  try {
    const raw = fs.readFileSync(getUiPreferencesPath(), 'utf-8')
    uiPreferences = normalizeUiPreferences(JSON.parse(raw) as Partial<UiPreferences>)
  } catch {
    uiPreferences = { ...DEFAULT_UI_PREFERENCES }
  }
  updateNativeThemeSource(uiPreferences.theme)
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

function getTrayTemplateIconPath() {
  return [
    path.join(__dirname, '../../build/trayTemplate.png'),
    path.join(__dirname, '../../public/trayTemplate.png'),
    path.join(__dirname, '../../dist/trayTemplate.png')
  ].find((candidate) => fs.existsSync(candidate))
}

function getWindowIconOptions() {
  const icon = getAppIconPath()
  return icon ? { icon } : {}
}

function requestQuitConfirmation() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.show()
    mainWindow.focus()
    mainWindow.webContents.send('app:window-close-request', { isQuit: true })
    return
  }

  isQuitting = true
  void ipcServices?.workspaceService.shutdown()
  app.quit()
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

function createTray() {
  const iconPath = isMac
    ? getTrayTemplateIconPath() ?? getAppIconPath()
    : getAppIconPath()
  if (!iconPath) {
    return
  }

  const image = nativeImage.createFromPath(iconPath)
  const trayImage = process.platform === 'darwin'
    ? image.resize({ width: 18, height: 18 })
    : image.resize({ width: 16, height: 16 })

  if (process.platform === 'darwin') {
    trayImage.setTemplateImage(true)
  }

  tray = new Tray(trayImage)
  const contextMenu = Menu.buildFromTemplate([
    {
      label: '显示主窗口',
      click: () => {
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.show()
          mainWindow.focus()
        } else {
          createMainWindow()
        }
      }
    },
    { type: 'separator' },
    {
      label: '退出',
      click: () => {
        requestQuitConfirmation()
      }
    }
  ])

  tray.setToolTip('TermDock')
  tray.setContextMenu(contextMenu)

  tray.on('click', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      if (mainWindow.isVisible()) {
        if (mainWindow.isFocused()) {
          mainWindow.hide()
        } else {
          mainWindow.focus()
        }
      } else {
        mainWindow.show()
        mainWindow.focus()
      }
    } else {
      createMainWindow()
    }
  })
}

function createMainWindow() {
  const win = new BrowserWindow({
    width: DEFAULT_WINDOW_BOUNDS.main.width,
    height: DEFAULT_WINDOW_BOUNDS.main.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.main.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.main.minHeight,
    center: true,
    title: 'TermDock',
    autoHideMenuBar: true,
    frame: isMac ? undefined : true,
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
  if (isWindows) {
    win.setMenuBarVisibility(false)
  }
  win.on('close', (event) => {
    if (isQuitting) {
      return
    }
    event.preventDefault()
    win.webContents.send('app:window-close-request', { isQuit: false })
  })

  win.on('closed', () => {
    if (mainWindow === win) {
      const childWindows = [
        connectionManagerWindow,
        connectionFormWindow,
        commandManagerWindow,
        commandFormWindow,
        fileEditorWindow
      ]
      for (const child of childWindows) {
        if (child && !child.isDestroyed()) {
          child.close()
        }
      }
      mainWindow = null
    }
  })

  if (!app.isPackaged) {
    loadAppWindow(win, undefined, uiPreferences)
    win.webContents.on('did-finish-load', async () => {
      try {
        const hasDesktopApi = await win.webContents.executeJavaScript('Boolean(window.termdock?.isDesktop)')
        appLog(`[TermDock] preload ready: ${hasDesktopApi}`)
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
  useVibrancy?: boolean
  visualEffectState?: 'followWindow' | 'active' | 'inactive'
  titleBarStyle?: 'default' | 'hidden' | 'hiddenInset' | 'customButtonsOnHover'
}) {
  const enableVibrancy = isMac && options.useVibrancy === true
  return new BrowserWindow({
    width: options.width,
    height: options.height,
    minWidth: options.minWidth,
    minHeight: options.minHeight,
    center: true,
    show: false,
    title: options.title,
    backgroundColor: options.backgroundColor ?? getWindowBackgroundColor(uiPreferences.theme),
    autoHideMenuBar: true,
    frame: isMac ? undefined : true,
    titleBarStyle: isMac ? options.titleBarStyle ?? 'hiddenInset' : 'default',
    trafficLightPosition: isMac ? { x: 16, y: 14 } : undefined,
    minimizable: false,
    vibrancy: enableVibrancy ? 'sidebar' : undefined,
    visualEffectState: enableVibrancy ? options.visualEffectState ?? 'active' : undefined,
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
  if (connectionManagerWindow && !connectionManagerWindow.isDestroyed()) {
    connectionManagerWindow.focus()
    return
  }

  const win = createNativeChildWindow({
    title: '连接管理器',
    width: DEFAULT_WINDOW_BOUNDS.connectionManager.width,
    height: DEFAULT_WINDOW_BOUNDS.connectionManager.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.connectionManager.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.connectionManager.minHeight
  })

  connectionManagerWindow = win
  attachWindowDiagnostics(win, 'connection-manager')
  win.once('ready-to-show', () => {
    centerChildWindowToParent(parent, win)
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
  if (commandManagerWindow && !commandManagerWindow.isDestroyed()) {
    commandManagerWindow.focus()
    return
  }

  const win = createNativeChildWindow({
    title: '命令管理器',
    width: DEFAULT_WINDOW_BOUNDS.commandManager.width,
    height: DEFAULT_WINDOW_BOUNDS.commandManager.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.commandManager.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.commandManager.minHeight,
    useVibrancy: false,
    visualEffectState: undefined,
    titleBarStyle: 'hiddenInset'
  })

  commandManagerWindow = win
  attachWindowDiagnostics(win, 'command-manager')
  win.once('ready-to-show', () => {
    centerChildWindowToParent(parent, win)
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
    width: DEFAULT_WINDOW_BOUNDS.connectionForm.width,
    height: DEFAULT_WINDOW_BOUNDS.connectionForm.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.connectionForm.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.connectionForm.minHeight
  })

  connectionFormWindow = win
  attachWindowDiagnostics(win, 'connection-form')
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
  if (commandFormWindow && !commandFormWindow.isDestroyed()) {
    commandFormWindow.close()
  }

  const win = createNativeChildWindow({
    title: mode === 'edit' ? '编辑命令' : '新建命令',
    width: DEFAULT_WINDOW_BOUNDS.commandForm.width,
    height: DEFAULT_WINDOW_BOUNDS.commandForm.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.commandForm.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.commandForm.minHeight,
    useVibrancy: false,
    visualEffectState: undefined,
    titleBarStyle: 'hiddenInset'
  })

  commandFormWindow = win
  attachWindowDiagnostics(win, `command-form:${mode}`)
  win.once('ready-to-show', () => {
    centerChildWindowToParent(parent, win)
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
  if (fileEditorWindow && !fileEditorWindow.isDestroyed()) {
    fileEditorWindow.close()
  }

  const win = createNativeChildWindow({
    title: `编辑文件 - ${input.name}`,
    width: DEFAULT_WINDOW_BOUNDS.fileEditor.width,
    height: DEFAULT_WINDOW_BOUNDS.fileEditor.height,
    minWidth: DEFAULT_WINDOW_BOUNDS.fileEditor.minWidth,
    minHeight: DEFAULT_WINDOW_BOUNDS.fileEditor.minHeight,
    backgroundColor: uiPreferences.theme === 'default-light' ? '#f8fafc' : '#171b20'
  })

  fileEditorWindow = win
  attachWindowDiagnostics(win, 'file-editor')
  win.once('ready-to-show', () => {
    centerChildWindowToParent(parent, win)
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
  initAppLogger(app.getPath('userData'))
  readUiPreferences()
  createTray()
  const appIconPath = getAppIconPath()
  if (isMac && appIconPath) {
    app.dock?.setIcon(appIconPath)
  }
  ipcServices = registerIpcHandlers(app.getPath('userData'), {
    getMainWindow: () => mainWindow,
    getUiPreferences: () => uiPreferences,
    setUiPreferences: (input) => {
      const next = updateUiPreferences(input)
      updateNativeThemeSource(next.theme)
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
    openFileEditorWindow,
    openLogsDirectory,
    confirmCloseWindow: (action) => {
      if (action === 'quit') {
        isQuitting = true
        const childWindows = [
          connectionManagerWindow,
          connectionFormWindow,
          commandManagerWindow,
          commandFormWindow,
          fileEditorWindow
        ]
        for (const child of childWindows) {
          if (child && !child.isDestroyed()) {
            child.close()
          }
        }
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.close()
        }
        app.quit()
      } else if (action === 'hide') {
        const childWindows = [
          connectionManagerWindow,
          connectionFormWindow,
          commandManagerWindow,
          commandFormWindow,
          fileEditorWindow
        ]
        for (const child of childWindows) {
          if (child && !child.isDestroyed()) {
            child.close()
          }
        }
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.hide()
        }
      }
    }
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

app.on('before-quit', (event) => {
  if (isQuitting) {
    void ipcServices?.workspaceService.shutdown()
    return
  }

  event.preventDefault()
  requestQuitConfirmation()
})
