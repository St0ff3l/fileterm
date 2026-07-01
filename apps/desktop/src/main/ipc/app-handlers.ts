import { BrowserWindow, ipcMain, shell, Menu } from 'electron'
import type { ConnectionFormMode, FileEditorWindowInput } from '@fileterm/core'
import type { IpcWindowOptions } from './types.js'

export function registerAppHandlers(options: IpcWindowOptions) {
  ipcMain.handle('app:getUiPreferences', () => options.getUiPreferences())

  ipcMain.handle('app:setUiPreferences', (_event, input: Partial<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>) => {
    return options.setUiPreferences(input)
  })

  ipcMain.handle('app:getUiStateItem', async (_event, key: string) => {
    return options.getUiStateItem(key)
  })

  ipcMain.handle('app:setUiStateItem', async (_event, key: string, value: string) => {
    await options.setUiStateItem(key, value)
  })

  ipcMain.handle('app:removeUiStateItem', async (_event, key: string) => {
    await options.removeUiStateItem(key)
  })

  ipcMain.handle('app:openConnectionManagerWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openConnectionManagerWindow(senderWindow)
    }
  })

  ipcMain.handle('app:openCommandManagerWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openCommandManagerWindow(senderWindow)
    }
  })

  ipcMain.handle('app:openConnectionFormWindow', (event, mode: ConnectionFormMode, profileId?: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openConnectionFormWindow(senderWindow, mode, profileId)
    }
  })

  ipcMain.handle('app:openCommandFormWindow', (event, mode: ConnectionFormMode, commandId?: string, folderId?: string) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openCommandFormWindow(senderWindow, mode, commandId, folderId)
    }
  })

  ipcMain.handle('app:openFileEditorWindow', (event, input: FileEditorWindowInput) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender) ?? options.getMainWindow()
    if (senderWindow) {
      options.openFileEditorWindow(senderWindow, input)
    }
  })

  ipcMain.handle('app:openExternalUrl', async (_event, rawUrl: string) => {
    await openExternalUrl(rawUrl)
  })

  ipcMain.handle('app:openLogsDirectory', () => options.openLogsDirectory())

  ipcMain.handle('app:minimizeCurrentWindow', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.handle('app:isCurrentWindowMaximized', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    return win ? win.isMaximized() : false
  })

  ipcMain.handle('app:toggleMaximizeCurrentWindow', (event) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    if (!senderWindow) {
      return
    }
    if (senderWindow.isMaximized()) {
      senderWindow.unmaximize()
      return
    }
    senderWindow.maximize()
  })

  ipcMain.handle('app:closeCurrentWindow', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('app:showWindowMenu', (event, menuType: 'app' | 'file' | 'view' | 'window', x: number, y: number) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    if (!senderWindow) {
      return
    }
    const menu = getWindowMenu(senderWindow, menuType, options)
    menu.popup({ window: senderWindow, x, y })
  })

  ipcMain.handle('app:requestQuitApp', () => {
    options.requestQuitApp()
  })

  ipcMain.handle('app:confirmCloseWindow', (_event, action: 'quit' | 'hide' | 'cancel') => {
    options.confirmCloseWindow(action)
  })
}

function getWindowMenu(
  senderWindow: BrowserWindow,
  menuType: 'app' | 'file' | 'view' | 'window',
  options: IpcWindowOptions
): Menu {
  const isEn = options.getUiPreferences().locale === 'enUS'

  if (menuType === 'app') {
    return Menu.buildFromTemplate([
      {
        label: 'Version 1.2.0-stable',
        click: () => {}
      }
    ])
  } else if (menuType === 'file') {
    return Menu.buildFromTemplate([
      {
        label: isEn ? 'New Connection' : '新建连接',
        accelerator: 'Ctrl+N',
        click: () => {
          options.openConnectionFormWindow(senderWindow, 'create')
        }
      },
      {
        label: isEn ? 'Connection Manager' : '连接管理',
        accelerator: 'Ctrl+Shift+C',
        click: () => {
          options.openConnectionManagerWindow(senderWindow)
        }
      },
      {
        label: isEn ? 'Command Manager' : '命令管理',
        accelerator: 'Ctrl+Shift+M',
        click: () => {
          options.openCommandManagerWindow(senderWindow)
        }
      },
      { type: 'separator' },
      {
        label: isEn ? 'Open Logs Directory' : '打开日志目录',
        click: () => {
          void options.openLogsDirectory()
        }
      },
      { type: 'separator' },
      {
        label: isEn ? 'Exit' : '退出',
        accelerator: 'Alt+F4',
        click: () => {
          options.requestQuitApp()
        }
      }
    ])
  } else if (menuType === 'view') {
    return Menu.buildFromTemplate([
      {
        label: isEn ? 'Reload' : '重新加载',
        accelerator: 'F5',
        click: () => {
          senderWindow.webContents.reload()
        }
      },
      {
        label: isEn ? 'Toggle Developer Tools' : '开发者工具',
        accelerator: 'F12',
        click: () => {
          senderWindow.webContents.toggleDevTools()
        }
      },
      { type: 'separator' },
      {
        label: isEn ? 'Reset Zoom' : '实际大小',
        accelerator: 'Ctrl+0',
        click: () => {
          senderWindow.webContents.setZoomLevel(0)
        }
      },
      {
        label: isEn ? 'Zoom In' : '放大',
        accelerator: 'Ctrl+Plus',
        click: () => {
          const currentLevel = senderWindow.webContents.getZoomLevel()
          senderWindow.webContents.setZoomLevel(currentLevel + 0.5)
        }
      },
      {
        label: isEn ? 'Zoom Out' : '缩小',
        accelerator: 'Ctrl+-',
        click: () => {
          const currentLevel = senderWindow.webContents.getZoomLevel()
          senderWindow.webContents.setZoomLevel(currentLevel - 0.5)
        }
      }
    ])
  } else {
    return Menu.buildFromTemplate([
      {
        label: isEn ? 'Minimize' : '最小化',
        click: () => {
          senderWindow.minimize()
        }
      },
      {
        label: senderWindow.isMaximized()
          ? (isEn ? 'Restore' : '还原')
          : (isEn ? 'Maximize' : '最大化'),
        click: () => {
          if (senderWindow.isMaximized()) {
            senderWindow.unmaximize()
          } else {
            senderWindow.maximize()
          }
        }
      },
      { type: 'separator' },
      {
        label: isEn ? 'Close Window' : '关闭窗口',
        accelerator: 'Ctrl+W',
        click: () => {
          senderWindow.close()
        }
      }
    ])
  }
}

async function openExternalUrl(rawUrl: string) {
  const url = new URL(rawUrl)
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error(`Unsupported external URL protocol: ${url.protocol}`)
  }
  await shell.openExternal(url.toString())
}
