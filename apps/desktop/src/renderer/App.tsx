import { useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandTemplateInput,
  ConnectionFormMode,
  ConnectionProfile,
  CreateProfileInput,
  FileContentSnapshot,
  LocalFileItem,
  PermissionChangeOptions,
  RemoteFileItem,
  SshCredentialsPromptRequest,
  SshHostVerificationRequest,
  SshInteractionRequest,
  SshInteractionResponse,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { defaultForm, emptyState, localPreviewFiles, previewLocalPath, previewState, profileToForm } from './app/app-data'
import { homeTabKey, insertTabKeyAfter, isActiveTransfer, reorderTabKeys, sessionTabKey, withParentRow } from './app/app-utils'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from './features/commands/CommandEditorModal'
import { CommandManagerModal } from './features/commands/CommandManagerModal'
import { ConnectionManagerModal } from './features/connections/ConnectionManagerModal'
import { ConnectionModal } from './features/connections/ConnectionModal'
import { SshCredentialsModal } from './features/connections/SshCredentialsModal'
import { SshHostVerificationModal } from './features/connections/SshHostVerificationModal'
import { FileActionModal } from './features/files/FileActionModal'
import { FileEditorModal } from './features/files/FileEditorModal'
import { FilePermissionModal } from './features/files/FilePermissionModal'
import { RootAccessModal } from './features/files/RootAccessModal'
import { AppIcon } from './features/common/AppIcon'
import { TabBar, type OrderedTabEntry, type TabContextTarget } from './features/layout/TabBar'
import { TabContextMenu } from './features/layout/TabContextMenu'
import { SystemSidebar } from './features/system/SystemSidebar'
import { TransferBar } from './features/transfers/TransferBar'
import { TransferPopover } from './features/transfers/TransferPopover'
import { WorkspaceStage } from './features/workspace/WorkspaceStage'
import { useThemeMode, type ThemeMode } from './hooks/useThemeMode'
import { defaultLocale, setLocale, t, type AppLocale } from './i18n'

const STATUS_MESSAGE_TIMEOUT_MS = 15_000
const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i
const THEME_STORAGE_KEY = 'termdock.theme'
const LOCALE_STORAGE_KEY = 'termdock.locale'
const MAIN_TAB_UI_STORAGE_KEY = 'termdock.main.tab-ui'

type ErrorDetails = {
  item?: RemoteFileItem
  targetPath?: string
}

type LocalTab =
  | { id: string; kind: 'home'; title: string }
  | { id: string; kind: 'system'; title: string; sessionTabId: string; sourceTabTitle: string }

type StoredMainTabUiState = {
  localTabs: LocalTab[]
  activeLocalTabId: string | null
  nextHomeTabNumber: number
  tabOrder: string[]
}

function formatSystemInfoTabTitle(sourceTabTitle: string) {
  return `${t.systemInfoTabTitle} · ${sourceTabTitle || t.untitledTab}`
}

type FileDialogTarget = {
  pane: 'local' | 'remote'
  path: string
  name: string
  type: 'file' | 'folder'
}

type FileClipboardState = {
  pane: 'local' | 'remote'
  operation: 'copy' | 'cut'
  items: FileDialogTarget[]
  tabId?: string
}

function areClipboardItemsEqual(left: FileDialogTarget[], right: FileDialogTarget[]) {
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    const leftItem = left[index]
    const rightItem = right[index]
    if (
      !leftItem
      || !rightItem
      || leftItem.pane !== rightItem.pane
      || leftItem.path !== rightItem.path
      || leftItem.name !== rightItem.name
      || leftItem.type !== rightItem.type
    ) {
      return false
    }
  }

  return true
}

type FileActionDialog =
  | { kind: 'new-folder'; pane: 'local' | 'remote'; directoryPath: string }
  | { kind: 'new-file'; pane: 'local' | 'remote'; directoryPath: string }
  | { kind: 'rename'; target: FileDialogTarget }
  | { kind: 'delete'; targets: FileDialogTarget[] }

function splitNameForDuplicate(name: string, type: 'file' | 'folder') {
  if (type === 'folder') {
    return { stem: name, ext: '' }
  }

  const dotIndex = name.lastIndexOf('.')
  if (dotIndex <= 0 || dotIndex === name.length - 1) {
    return { stem: name, ext: '' }
  }

  return {
    stem: name.slice(0, dotIndex),
    ext: name.slice(dotIndex)
  }
}

function makeDuplicateName(name: string, type: 'file' | 'folder', attempt: number) {
  const { stem, ext } = splitNameForDuplicate(name, type)
  const suffix = attempt === 1 ? ' copy' : ` copy ${attempt}`
  return `${stem}${suffix}${ext}`
}

function allocateTargetNames(
  items: FileDialogTarget[],
  existingNames: string[],
  operation: 'copy' | 'cut',
  destinationPath: string
) {
  const reservedNames = new Set(existingNames)
  return items.map((item) => {
    const isSameDirectory = item.pane === 'remote'
      ? remoteDirname(item.path) === destinationPath
      : localDirname(item.path) === destinationPath

    let nextName = item.name

    if (operation === 'cut' && isSameDirectory) {
      reservedNames.add(nextName)
      return nextName
    }

    if (reservedNames.has(nextName) || (operation === 'copy' && isSameDirectory)) {
      let attempt = 1
      do {
        nextName = makeDuplicateName(item.name, item.type, attempt)
        attempt += 1
      } while (reservedNames.has(nextName))
    }

    reservedNames.add(nextName)
    return nextName
  })
}

function remoteDirname(targetPath: string) {
  const normalized = targetPath.replace(/\/+$/, '') || '/'
  if (normalized === '/') {
    return '/'
  }
  const slashIndex = normalized.lastIndexOf('/')
  if (slashIndex <= 0) {
    return '/'
  }
  return normalized.slice(0, slashIndex)
}

function joinRemotePath(directoryPath: string, name: string) {
  return directoryPath === '/' ? `/${name}` : `${directoryPath.replace(/\/+$/, '')}/${name}`
}

function normalizeLocalPath(targetPath: string) {
  return targetPath.replace(/[\\/]+$/, '')
}

function localDirname(targetPath: string) {
  const normalized = normalizeLocalPath(targetPath)
  if (/^[A-Za-z]:$/.test(normalized)) {
    return `${normalized}\\`
  }
  const slashIndex = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'))
  if (slashIndex <= 0) {
    return slashIndex === 0 ? normalized.slice(0, 1) : '.'
  }
  if (slashIndex === 2 && /^[A-Za-z]:/.test(normalized)) {
    return normalized.slice(0, 3)
  }
  return normalized.slice(0, slashIndex)
}

function joinLocalPath(directoryPath: string, name: string) {
  const separator = directoryPath.includes('\\') ? '\\' : '/'
  const normalized = normalizeLocalPath(directoryPath)
  if (normalized === separator) {
    return `${separator}${name}`
  }
  return `${normalized}${separator}${name}`
}

function readStoredLocale(): AppLocale {
  if (typeof window === 'undefined') {
    return defaultLocale
  }

  try {
    const nextLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY)
    return nextLocale === 'enUS' || nextLocale === 'zhCN' ? nextLocale : defaultLocale
  } catch {
    return defaultLocale
  }
}

function readStoredMainTabUiState(enabled: boolean): StoredMainTabUiState | null {
  if (!enabled || typeof window === 'undefined') {
    return null
  }

  try {
    const raw = window.localStorage.getItem(MAIN_TAB_UI_STORAGE_KEY)
    if (!raw) {
      return null
    }
    const parsed = JSON.parse(raw) as Partial<StoredMainTabUiState>
    const localTabs = Array.isArray(parsed.localTabs)
      ? parsed.localTabs.filter((tab): tab is LocalTab => {
          if (!tab || typeof tab !== 'object' || typeof tab.id !== 'string' || typeof tab.title !== 'string') {
            return false
          }
          if (tab.kind === 'home') {
            return true
          }
          return tab.kind === 'system'
            && typeof (tab as Extract<LocalTab, { kind: 'system' }>).sessionTabId === 'string'
            && typeof (tab as Extract<LocalTab, { kind: 'system' }>).sourceTabTitle === 'string'
        })
      : []
    const tabOrder = Array.isArray(parsed.tabOrder)
      ? parsed.tabOrder.filter((entry): entry is string => typeof entry === 'string')
      : []

    return {
      localTabs,
      activeLocalTabId: typeof parsed.activeLocalTabId === 'string' ? parsed.activeLocalTabId : null,
      nextHomeTabNumber: typeof parsed.nextHomeTabNumber === 'number' && Number.isFinite(parsed.nextHomeTabNumber)
        ? Math.max(1, Math.floor(parsed.nextHomeTabNumber))
        : 1,
      tabOrder
    }
  } catch {
    return null
  }
}

function collectConnectionGroups(
  folderNames: string[],
  profileGroups: string[],
  currentGroup?: string
) {
  const next = new Set<string>()

  next.add('默认')

  for (const name of folderNames) {
    const value = name.trim()
    if (value) {
      next.add(value)
    }
  }

  for (const group of profileGroups) {
    const value = group.trim()
    if (value) {
      next.add(value)
    }
  }

  if (currentGroup?.trim()) {
    next.add(currentGroup.trim())
  }

  return [...next]
}

function writeStoredMainTabUiState(enabled: boolean, state: StoredMainTabUiState) {
  if (!enabled || typeof window === 'undefined') {
    return
  }

  try {
    window.localStorage.setItem(MAIN_TAB_UI_STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Ignore persistence failures; tab UI state should remain in-memory.
  }
}

function readInitialTheme(searchParams: URLSearchParams): ThemeMode {
  const queryTheme = searchParams.get('theme')
  if (queryTheme === 'default-light' || queryTheme === 'default-dark') {
    return queryTheme
  }

  if (typeof window === 'undefined') {
    return 'default-dark'
  }

  try {
    const nextTheme = window.localStorage.getItem(THEME_STORAGE_KEY)
    return nextTheme === 'default-light' || nextTheme === 'default-dark' ? nextTheme : 'default-dark'
  } catch {
    return 'default-dark'
  }
}

function readInitialLocale(searchParams: URLSearchParams): AppLocale {
  const queryLocale = searchParams.get('locale')
  if (queryLocale === 'enUS' || queryLocale === 'zhCN') {
    return queryLocale
  }

  return readStoredLocale()
}

function areStringArraysEqual(left: string[], right: string[]) {
  if (left === right) {
    return true
  }
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) {
      return false
    }
  }

  return true
}

function isDefaultPlaceholderHomeTab(tab: LocalTab) {
  return tab.kind === 'home' && tab.id === 'home-1' && tab.title === t.untitledTab
}

export function App() {
  const searchParams = new URLSearchParams(window.location.search)
  const windowMode = searchParams.get('window') ?? 'main'
  const isConnectionManagerWindow = windowMode === 'connection-manager'
  const isCommandManagerWindow = windowMode === 'command-manager'
  const isConnectionFormWindow = windowMode === 'connection-form'
  const isCommandFormWindow = windowMode === 'command-form'
  const isFileEditorWindow = windowMode === 'file-editor'
  const isMainWorkspaceWindow = !isConnectionManagerWindow
    && !isCommandManagerWindow
    && !isConnectionFormWindow
    && !isCommandFormWindow
    && !isFileEditorWindow
  const formWindowMode = (searchParams.get('mode') as ConnectionFormMode | null) ?? 'create'
  const formWindowProfileId = searchParams.get('profileId')
  const formWindowCommandId = searchParams.get('commandId')
  const formWindowFolderId = searchParams.get('folderId')
  const fileEditorWindowSource = searchParams.get('source') as FileContentSnapshot['source'] | null
  const fileEditorWindowPath = searchParams.get('path')
  const fileEditorWindowName = searchParams.get('name')
  const fileEditorWindowTabId = searchParams.get('tabId')
  const fileEditorWindowEncoding = searchParams.get('encoding') ?? 'utf-8'

  const [workspace, setWorkspace] = useState<WorkspaceSnapshot>(emptyState)
  const [error, setError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const [hasLoadedInitialSnapshot, setHasLoadedInitialSnapshot] = useState(false)
  const [showForm, setShowForm] = useState(false)
  const [showConnectionManager, setShowConnectionManager] = useState(false)
  const [showCommandManager, setShowCommandManager] = useState(false)
  const [form, setForm] = useState<CreateProfileInput>(defaultForm)
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null)
  const [localPath, setLocalPath] = useState(previewLocalPath)
  const [localItems, setLocalItems] = useState<LocalFileItem[]>(localPreviewFiles)
  const storedMainTabUiStateRef = useRef<StoredMainTabUiState | null>(null)
  if (storedMainTabUiStateRef.current === null) {
    storedMainTabUiStateRef.current = readStoredMainTabUiState(isMainWorkspaceWindow)
  }
  const storedMainTabUiState = storedMainTabUiStateRef.current
  const [localTabs, setLocalTabs] = useState<LocalTab[]>(() => storedMainTabUiState?.localTabs ?? [])
  const [activeLocalTabId, setActiveLocalTabId] = useState<string | null>(() => storedMainTabUiState?.activeLocalTabId ?? null)
  const [nextHomeTabNumber, setNextHomeTabNumber] = useState(() => storedMainTabUiState?.nextHomeTabNumber ?? 1)
  const [tabOrder, setTabOrder] = useState<string[]>(() => storedMainTabUiState?.tabOrder ?? [])
  const [draggingTabKey, setDraggingTabKey] = useState<string | null>(null)
  const [tabContextMenu, setTabContextMenu] = useState<{
    x: number
    y: number
    target: TabContextTarget
  } | null>(null)
  const [sidebarWidth, setSidebarWidth] = useState(214)
  const [isResizingSidebar, setIsResizingSidebar] = useState(false)
  const [fileEditor, setFileEditor] = useState<FileContentSnapshot | null>(
    isFileEditorWindow && fileEditorWindowSource && fileEditorWindowPath && fileEditorWindowName
      ? {
          source: fileEditorWindowSource,
          path: fileEditorWindowPath,
          name: fileEditorWindowName,
          tabId: fileEditorWindowTabId ?? undefined,
          encoding: fileEditorWindowEncoding,
          content: ''
        }
      : null
  )
  const [fileEditorError, setFileEditorError] = useState<string | null>(null)
  const [fileActionDialog, setFileActionDialog] = useState<FileActionDialog | null>(null)
  const [fileActionError, setFileActionError] = useState<string | null>(null)
  const [isFileActionSubmitting, setIsFileActionSubmitting] = useState(false)
  const [fileClipboard, setFileClipboard] = useState<FileClipboardState | null>(null)
  const [permissionDialog, setPermissionDialog] = useState<{
    target: FileDialogTarget & { permission?: string }
    supportsRecursive: boolean
  } | null>(null)
  const [permissionDialogError, setPermissionDialogError] = useState<string | null>(null)
  const [rootAccessDialog, setRootAccessDialog] = useState<{
    tabId: string
    sshUser?: string
    sudoUser: string
  } | null>(null)
  const connectionGroupOptions = useMemo(
    () => collectConnectionGroups(
      (workspace.folders ?? []).map((folder) => folder.name),
      workspace.profiles.map((profile) => profile.group),
      form.group
    ),
    [workspace.folders, workspace.profiles, form.group]
  )
  const [rootAccessDialogError, setRootAccessDialogError] = useState<string | null>(null)
  const [isRootAccessSubmitting, setIsRootAccessSubmitting] = useState(false)
  const [sshInteraction, setSshInteraction] = useState<SshInteractionRequest | null>(null)
  const [sshInteractionError, setSshInteractionError] = useState<string | null>(null)
  const [showTransfers, setShowTransfers] = useState(false)
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => readInitialTheme(searchParams))
  const [locale, setLocaleState] = useState<AppLocale>(() => readInitialLocale(searchParams))
  const [closeConfirmDialog, setCloseConfirmDialog] = useState<{ isQuit: boolean; hasActiveConnections: boolean } | null>(null)

  useThemeMode(themeMode)

  const workspaceRef = useRef(workspace)
  useEffect(() => {
    workspaceRef.current = workspace
  }, [workspace])

  const localTabsRef = useRef(localTabs)
  const previousActiveTransferCountRef = useRef(0)
  const pendingHomeReplacementKeyRef = useRef<string | null>(null)
  const hasSanitizedStoredPlaceholderRef = useRef(false)
  const desktopApi = window.termdock
  const isWindowsDesktop = false

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow) {
      return
    }

    const unsubscribe = desktopApi.onWindowCloseRequest((event) => {
      const hasActive = workspaceRef.current.tabs.some(
        (tab) => workspaceRef.current.sessions[tab.id]?.connected
      )

      if (desktopApi.platform === 'darwin') {
        if (event.isQuit) {
          setCloseConfirmDialog({ isQuit: true, hasActiveConnections: hasActive })
        } else {
          void desktopApi.confirmCloseWindow('hide')
        }
      } else {
        if (event.isQuit) {
          setCloseConfirmDialog({ isQuit: true, hasActiveConnections: hasActive })
        } else {
          setCloseConfirmDialog({ isQuit: false, hasActiveConnections: hasActive })
        }
      }
    })

    return () => unsubscribe()
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    document.documentElement.dataset.platform = desktopApi?.platform ?? 'browser'
  }, [desktopApi])

  useEffect(() => {
    localTabsRef.current = localTabs
  }, [localTabs])

  useEffect(() => {
    window.localStorage.setItem(THEME_STORAGE_KEY, themeMode)
    void desktopApi?.setUiPreferences({ theme: themeMode })
  }, [themeMode])

  useEffect(() => {
    setLocale(locale)
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale)
    void desktopApi?.setUiPreferences({ locale })
    setLocalTabs((prev) => {
      let changed = false
      const next = prev.map((tab) => {
        if (tab.kind === 'home') {
          if (tab.title === t.untitledTab) {
            return tab
          }
          changed = true
          return { ...tab, title: t.untitledTab }
        }
        const sourceTabTitle = workspace.tabs.find((entry) => entry.id === tab.sessionTabId)?.title ?? tab.sourceTabTitle
        const title = formatSystemInfoTabTitle(sourceTabTitle)
        if (tab.sourceTabTitle === sourceTabTitle && tab.title === title) {
          return tab
        }
        changed = true
        return {
          ...tab,
          sourceTabTitle,
          title
        }
      })
      return changed ? next : prev
    })
  }, [locale, workspace.tabs])

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === THEME_STORAGE_KEY) {
        const nextTheme = event.newValue
        if (nextTheme === 'default-dark' || nextTheme === 'default-light') {
          setThemeMode(nextTheme)
        }
      }

      if (event.key === LOCALE_STORAGE_KEY) {
        const nextLocale = event.newValue
        if (nextLocale === 'zhCN' || nextLocale === 'enUS') {
          setLocaleState(nextLocale)
        }
      }
    }

    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [])

  useEffect(() => {
    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
      setActiveLocalTabId(null)
      setHasLoadedInitialSnapshot(true)
      if (!isFileEditorWindow) {
        setTabOrder([])
        setError(t.browserPreview)
      }
      return
    }

    desktopApi
      .getSnapshot()
      .then((snapshot) => {
        setWorkspace(snapshot)
        if (isMainWorkspaceWindow && snapshot.tabs.length === 0) {
          setLocalTabs((current) => current.length ? current : [{ id: 'home-1', kind: 'home', title: t.untitledTab }])
          setActiveLocalTabId((current) => current ?? 'home-1')
          setTabOrder((current) => current.includes('home:home-1') ? current : ['home:home-1', ...current])
          setNextHomeTabNumber((current) => Math.max(current, 2))
        }
        setHasLoadedInitialSnapshot(true)
      })
      .catch((err: Error) => reportError(setError, '获取工作区快照', err))
      .finally(() => setHasLoadedInitialSnapshot(true))
  }, [desktopApi, isFileEditorWindow])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    const offSnapshot = desktopApi.onWorkspaceSnapshot((snapshot) => {
      applySnapshot(snapshot)
    })

    if (!isConnectionManagerWindow && !isConnectionFormWindow && !isCommandManagerWindow && !isCommandFormWindow && !isFileEditorWindow) {
      desktopApi
        .listLocalDirectory()
        .then(({ path, items }) => {
          setLocalPath(path)
          setLocalItems(withParentRow(path, items))
        })
        .catch(() => setError(t.localLoadFailed))
    }

    return () => {
      offSnapshot()
    }
  }, [desktopApi, isCommandFormWindow, isCommandManagerWindow, isConnectionFormWindow, isConnectionManagerWindow, isFileEditorWindow])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    return desktopApi.onSshInteraction((request) => {
      setSshInteraction(request)
      setSshInteractionError(null)
    })
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi || !isFileEditorWindow || !fileEditorWindowSource || !fileEditorWindowPath || !fileEditorWindowName) {
      return
    }

    void (async () => {
    try {
      setIsBusy(true)
      const content = fileEditorWindowSource === 'local'
        ? await desktopApi.readLocalFile(fileEditorWindowPath, fileEditorWindowEncoding)
        : fileEditorWindowTabId
            ? await desktopApi.readRemoteFile(fileEditorWindowTabId, fileEditorWindowPath, fileEditorWindowEncoding)
            : ''

        setFileEditor({
          source: fileEditorWindowSource,
          path: fileEditorWindowPath,
          name: fileEditorWindowName,
          tabId: fileEditorWindowTabId ?? undefined,
          encoding: fileEditorWindowEncoding,
          content
        })
        setFileEditorError(null)
      } catch (err) {
        reportError(setFileEditorError, '打开文件编辑器', err)
      } finally {
        setIsBusy(false)
      }
    })()
  }, [desktopApi, fileEditorWindowEncoding, fileEditorWindowName, fileEditorWindowPath, fileEditorWindowSource, fileEditorWindowTabId, isFileEditorWindow])

  useEffect(() => {
    if (!isConnectionFormWindow) {
      return
    }

    if (formWindowMode === 'edit') {
      const profile = workspace.profiles.find((item) => item.id === formWindowProfileId)
      if (!profile) {
        setFormError(t.profileNotFound)
        return
      }
      setEditingProfileId(profile.id)
      setForm(profileToForm(profile))
      setFormError(null)
      return
    }

    setEditingProfileId(null)
    setForm(defaultForm)
    setFormError(null)
  }, [formWindowMode, formWindowProfileId, isConnectionFormWindow, workspace.profiles])

  useEffect(() => {
    const activeTransferCount = workspace.transfers.filter(isActiveTransfer).length
    if (activeTransferCount > previousActiveTransferCountRef.current) {
      setShowTransfers(true)
    }
    previousActiveTransferCountRef.current = activeTransferCount
  }, [workspace.transfers])

  useEffect(() => {
    const allKeys = [
      ...localTabs.map((tab) => homeTabKey(tab.id)),
      ...workspace.tabs.map((tab) => sessionTabKey(tab.id))
    ]

    setTabOrder((prev) => {
      const kept = prev.filter((key) => allKeys.includes(key))
      const missing = allKeys.filter((key) => !kept.includes(key))
      const replacementKey = pendingHomeReplacementKeyRef.current

      if (replacementKey && missing.length) {
        const replaceIndex = kept.indexOf(replacementKey)
        if (replaceIndex !== -1) {
          const next = [...kept]
          next.splice(replaceIndex, 1, missing[0])
          pendingHomeReplacementKeyRef.current = null
          const nextOrder = [...next, ...missing.slice(1)]
          return areStringArraysEqual(prev, nextOrder) ? prev : nextOrder
        }
      }

      const nextOrder = [...kept, ...missing]
      return areStringArraysEqual(prev, nextOrder) ? prev : nextOrder
    })
  }, [localTabs, workspace.tabs])

  useEffect(() => {
    if (!hasLoadedInitialSnapshot || localTabs.length > 0 || workspace.tabs.length > 0) {
      return
    }

    setLocalTabs([{ id: 'home-1', kind: 'home', title: t.untitledTab }])
    setActiveLocalTabId((current) => current ?? 'home-1')
    setTabOrder((prev) => prev.includes('home:home-1') ? prev : ['home:home-1', ...prev])
    setNextHomeTabNumber((prev) => Math.max(prev, 2))
  }, [hasLoadedInitialSnapshot, localTabs.length, workspace.tabs.length])

  useEffect(() => {
    if (!hasLoadedInitialSnapshot) {
      return
    }

    if (!hasSanitizedStoredPlaceholderRef.current) {
      hasSanitizedStoredPlaceholderRef.current = true
      const onlyPlaceholderHomeTab = localTabs.length === 1 && isDefaultPlaceholderHomeTab(localTabs[0]!)
      const hasRemoteSessions = workspace.tabs.length > 0
      const isPlaceholderInactive = activeLocalTabId === null

      if (onlyPlaceholderHomeTab && hasRemoteSessions && isPlaceholderInactive) {
        setLocalTabs([])
        setTabOrder((prev) => prev.filter((key) => key !== 'home:home-1'))
        setNextHomeTabNumber(1)
        return
      }
    }

    const validSessionTabIds = new Set(workspace.tabs.map((tab) => tab.id))
    const nextLocalTabs = localTabs.filter((tab) => tab.kind === 'home' || validSessionTabIds.has(tab.sessionTabId))
    if (nextLocalTabs.length !== localTabs.length) {
      setLocalTabs(nextLocalTabs)
    }
    setActiveLocalTabId((prev) => {
      if (!prev) {
        return prev
      }
      return nextLocalTabs.some((tab) => tab.id === prev)
        ? prev
        : null
    })
  }, [activeLocalTabId, hasLoadedInitialSnapshot, localTabs, workspace.tabs])

  useEffect(() => {
    if (!hasLoadedInitialSnapshot) {
      return
    }

    writeStoredMainTabUiState(isMainWorkspaceWindow, {
      localTabs,
      activeLocalTabId,
      nextHomeTabNumber,
      tabOrder
    })
  }, [activeLocalTabId, hasLoadedInitialSnapshot, isMainWorkspaceWindow, localTabs, nextHomeTabNumber, tabOrder])

  useEffect(() => {
    if (!isResizingSidebar) {
      return
    }

    const onMouseMove = (event: globalThis.MouseEvent) => {
      setSidebarWidth(Math.min(360, Math.max(190, event.clientX)))
    }

    const onMouseUp = () => {
      setIsResizingSidebar(false)
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isResizingSidebar])

  useEffect(() => {
    if (!error) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setError((current) => current === error ? null : current)
    }, STATUS_MESSAGE_TIMEOUT_MS)

    return () => {
      window.clearTimeout(timeoutId)
    }
  }, [error])

  useEffect(() => {
    if (!fileClipboard) {
      return
    }

    const handleEscapeClearClipboard = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return
      }
      setFileClipboard(null)
    }

    window.addEventListener('keydown', handleEscapeClearClipboard)
    return () => window.removeEventListener('keydown', handleEscapeClearClipboard)
  }, [fileClipboard])

  const activeLocalTab = activeLocalTabId ? localTabs.find((tab) => tab.id === activeLocalTabId) ?? null : null
  const displayedSessionTabId = activeLocalTab
    ? activeLocalTab.kind === 'system' ? activeLocalTab.sessionTabId : null
    : workspace.activeTabId
  const activeTab = displayedSessionTabId ? workspace.tabs.find((tab) => tab.id === displayedSessionTabId) ?? null : null
  const activeSession = activeTab ? workspace.sessions[activeTab.id] : null
  const activeProfile = activeTab
    ? workspace.profiles.find((profile) => profile.id === activeTab.profileId) ?? null
    : null
  const activeTransferCount = workspace.transfers.filter(isActiveTransfer).length
  const showSidebar = activeTab !== null && activeSession !== null && activeLocalTab?.kind !== 'home'

  const normalizeErrorMessage = (err: unknown) => {
    const rawMessage = err instanceof Error ? err.message : String(err)
    return rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()
  }

  const formatAppError = (scope: string, err: unknown, details?: ErrorDetails) => {
    const message = normalizeErrorMessage(err)
    const likelyConcurrentRequestIssue = /another one is still running|forgot to use 'await'|client is closed because user launched a task/i.test(message)
    const likelyPathIssue = /can't cd to|__NOT_DIR__|no such file|not a directory|permission denied|\b550\b/i.test(message)
    const metadata = details?.item
      ? ` (${t.permission}: ${details.item.permission || '-'}, ${t.ownerGroup}: ${details.item.ownerGroup || '-'})`
      : ''
    const pathText = details?.targetPath ? ` ${details.targetPath}` : ''

    if (locale === 'zhCN') {
      if (details?.targetPath && likelyConcurrentRequestIssue) {
        return `打开远程目录${pathText}${metadata}失败：远程连接正在处理另一项请求，请稍后重试。原始错误：${message}`
      }
      if (details?.targetPath && likelyPathIssue) {
        return `无法打开远程目录${pathText}${metadata}。可能是目录不存在、不是目录，或者当前账号没有进入权限。原始错误：${message}`
      }
      return `${scope}${pathText}${metadata}失败：${message}`
    }

    if (details?.targetPath && likelyConcurrentRequestIssue) {
      return `Failed to open remote directory${pathText}${metadata}: the remote connection is still processing another request. Raw error: ${message}`
    }
    if (details?.targetPath && likelyPathIssue) {
      return `Could not open remote directory${pathText}${metadata}. It may not exist, may not be a directory, or your account may not have permission to enter it. Raw error: ${message}`
    }

    return `${scope}${pathText}${metadata} failed: ${message}`
  }

  const reportError = (setter: (message: string) => void, scope: string, err: unknown, details?: ErrorDetails) => {
    console.error(`[TermDock] ${scope}`, err)
    setter(formatAppError(scope, err, details))
  }

  const shouldPromptForRootAccess = (err: unknown) => {
    const message = normalizeErrorMessage(err)
    return /未检测到可复用的 sudo 授权|sudo 密码错误|sudo 密码无效|sudo credentials|incorrect password|authentication failure/i.test(message)
  }

  const applySnapshot = (snapshot: WorkspaceSnapshot) => {
    setWorkspace(snapshot)
    setFormError(null)
  }

  const updateForm = (
    updater: CreateProfileInput | ((prev: CreateProfileInput) => CreateProfileInput)
  ) => {
    setForm((prev) => (typeof updater === 'function' ? updater(prev) : updater))
    setFormError(null)
  }

  const closeCurrentWindow = () => {
    void desktopApi?.closeCurrentWindow()
  }

  const openCreateModal = () => {
    setEditingProfileId(null)
    setForm(defaultForm)
    setFormError(null)
    setShowForm(true)
  }

  const openEditModal = (profile: ConnectionProfile) => {
    setEditingProfileId(profile.id)
    setForm(profileToForm(profile))
    setFormError(null)
    setShowForm(true)
  }

  const openCreateConnection = () => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('create')
      return
    }
    openCreateModal()
  }

  const openEditConnection = (profile: ConnectionProfile) => {
    if (desktopApi) {
      void desktopApi.openConnectionFormWindow('edit', profile.id)
      return
    }
    openEditModal(profile)
  }

  const handleSaveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (!form.name || !form.host || !form.group || !form.remotePath || !Number(form.port)) {
      setFormError(t.fillRequired)
      return
    }

    if (form.type === 'ssh' && form.authType === 'privateKey' && !form.privateKeyPath) {
      setFormError(t.missingPrivateKeyPath)
      return
    }

    if (!desktopApi) {
      setFormError(t.desktopOnlyCreate)
      return
    }

    try {
      setIsBusy(true)
      const payload = { ...form, port: Number(form.port) }
      const snapshot = editingProfileId
        ? await desktopApi.updateProfile(editingProfileId, payload)
        : await desktopApi.createProfile(payload)
      applySnapshot(snapshot)
      if (isConnectionFormWindow) {
        closeCurrentWindow()
        return
      }
      setShowForm(false)
      setEditingProfileId(null)
      setForm(defaultForm)
    } catch (err) {
      reportError(setFormError, '保存连接', err)
    } finally {
      setIsBusy(false)
    }
  }

  const openCommandManager = () => {
    if (desktopApi) {
      void desktopApi.openCommandManagerWindow()
      return
    }
    setShowCommandManager(true)
  }

  const saveCommandTemplate = async (commandId: string | null, input: CommandTemplateInput) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = commandId
        ? await desktopApi.updateCommandTemplate(commandId, input)
        : await desktopApi.createCommandTemplate(input)
      applySnapshot(snapshot)
      if (isCommandFormWindow) {
        closeCurrentWindow()
      }
    } catch (err) {
      reportError(setError, '保存命令模板', err)
    } finally {
      setIsBusy(false)
    }
  }

  const createCommandFolder = async (name: string) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.createCommandFolder(name)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '新建命令分类', err)
    } finally {
      setIsBusy(false)
    }
  }

  const updateCommandFolder = async (folderId: string, updates: { name?: string; parentId?: string; order?: number }) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.updateCommandFolder(folderId, updates)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '更新命令分类', err)
    } finally {
      setIsBusy(false)
    }
  }

  const updateCommandOrder = async (id: string, parentId: string | undefined, order: number) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.updateCommandOrder(id, parentId, order)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '调整命令顺序', err)
    } finally {
      setIsBusy(false)
    }
  }

  const deleteCommandFolder = async (folderId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.deleteCommandFolder(folderId)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '删除命令分类', err)
    } finally {
      setIsBusy(false)
    }
  }

  const deleteCommandTemplate = async (commandId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.deleteCommandTemplate(commandId)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '删除命令模板', err)
    } finally {
      setIsBusy(false)
    }
  }

  const executeCommandTemplate = async (
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: 'current' | 'all-ssh'
  ) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const targetTabs = scope === 'all-ssh'
        ? workspace.tabs.filter((tab) => tab.sessionType === 'ssh' && tab.status !== 'closed')
        : activeTab && activeTab.sessionType === 'ssh'
          ? [activeTab]
          : []

      for (const tab of targetTabs) {
        await desktopApi.executeCommandTemplate(tab.id, commandId, args, options)
      }
    } catch (err) {
      reportError(setError, '执行命令模板', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleOpenProfile = async (profileId: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = activeLocalTabId
    const replacementKey = activeHomeId ? homeTabKey(activeHomeId) : null
    pendingHomeReplacementKeyRef.current = replacementKey

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.openProfile(profileId)
      setWorkspace(snapshot)
      setError(null)
      setFormError(null)
      if (activeHomeId && snapshot.activeTabId && replacementKey) {
        const nextSessionKey = sessionTabKey(snapshot.activeTabId)
        setTabOrder((prev) => prev.map((key) => key === replacementKey ? nextSessionKey : key))
        setLocalTabs((prev) => prev.filter((tab) => tab.id !== activeHomeId))
        pendingHomeReplacementKeyRef.current = null
      }
      setActiveLocalTabId(null)
    } catch (err) {
      pendingHomeReplacementKeyRef.current = null
      reportError(setError, '打开连接', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleDeleteProfile = async (profileId: string) => {
    if (!desktopApi) {
      setError(t.desktopOnlyDelete)
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.deleteProfile(profileId)
      applySnapshot(snapshot)
    } catch (err) {
      reportError(setError, '删除连接', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleClearHostFingerprint = async (profile: ConnectionProfile) => {
    if (!desktopApi || profile.type !== 'ssh') {
      return
    }

    try {
      setIsBusy(true)
      const nextInput: CreateProfileInput = {
        ...profileToForm(profile),
        trustedHostFingerprint: ''
      }
      const snapshot = await desktopApi.updateProfile(profile.id, nextInput)
      applySnapshot(snapshot)
      setError(null)
    } catch (err) {
      reportError(setError, '清除主机指纹', err)
    } finally {
      setIsBusy(false)
    }
  }

  const resolveSshInteraction = async (requestId: string, response: SshInteractionResponse) => {
    if (!desktopApi) {
      return
    }

    try {
      await desktopApi.resolveSshInteraction(requestId, response)
    } catch (err) {
      reportError(setError, '响应 SSH 交互', err)
    } finally {
      setSshInteraction((current) => current?.requestId === requestId ? null : current)
      setSshInteractionError(null)
    }
  }

  const handleSubmitSshCredentials = (request: SshCredentialsPromptRequest, input: { username: string; password: string }) => {
    const username = input.username.trim()
    const password = input.password

    if (!username || !password) {
      setSshInteractionError(t.sshAuthPromptFillRequired)
      return
    }

    void resolveSshInteraction(request.requestId, {
      kind: 'credentials',
      canceled: false,
      username,
      password
    })
  }

  const handleActivateTab = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.activateTab(tabId)
      applySnapshot(snapshot)
      setActiveLocalTabId(null)
    } catch (err) {
      reportError(setError, '激活标签页', err)
    } finally {
      setIsBusy(false)
    }
  }

  const closeSessionTabById = async (tabId: string) => {
    if (!desktopApi) {
      return null
    }

    const snapshot = await desktopApi.closeTab(tabId)
    applySnapshot(snapshot)
    const relatedLocalTabs = localTabsRef.current.filter((tab) => tab.kind === 'system' && tab.sessionTabId === tabId).map((tab) => tab.id)
    if (relatedLocalTabs.length) {
      closeHomeTabs(relatedLocalTabs, activeLocalTabId && relatedLocalTabs.includes(activeLocalTabId) ? null : activeLocalTabId, snapshot.tabs)
    }
    if (snapshot.activeTabId === null) {
      setLocalTabs((prev) => prev.length ? prev : [{ id: 'home-1', kind: 'home', title: t.untitledTab }])
      setTabOrder((prev) => {
        const filtered = prev.filter((key) => key !== sessionTabKey(tabId))
        return filtered.some((key) => key.startsWith('home:')) ? filtered : ['home:home-1', ...filtered]
      })
      setActiveLocalTabId((prev) => prev ?? localTabsRef.current.at(-1)?.id ?? 'home-1')
    }
    return snapshot
  }

  const handleCloseTab = async (event: MouseEvent<HTMLButtonElement>, tabId: string) => {
    event.stopPropagation()
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      await closeSessionTabById(tabId)
    } catch (err) {
      reportError(setError, '关闭标签页', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleActivateHome = (homeTabId: string) => {
    setError(null)
    setActiveLocalTabId(homeTabId)
  }

  const handleAddHomeTab = () => {
    const nextId = `home-${nextHomeTabNumber}`
    const nextKey = homeTabKey(nextId)

    setLocalTabs((prev) => [...prev, { id: nextId, kind: 'home', title: t.untitledTab }])
    setTabOrder((prev) => [...prev, nextKey])
    setNextHomeTabNumber((prev) => prev + 1)
    setActiveLocalTabId(nextId)
    setError(null)
  }

  const handleOpenSystemInfo = () => {
    if (!activeTab) {
      return
    }

    const existing = localTabs.find((tab) => tab.kind === 'system' && tab.sessionTabId === activeTab.id)
    if (existing) {
      setActiveLocalTabId(existing.id)
      setError(null)
      return
    }

    const nextId = `system-${activeTab.id}`
    const activeOrderKey = activeLocalTabId ? homeTabKey(activeLocalTabId) : sessionTabKey(activeTab.id)
    setLocalTabs((prev) => [
      ...prev,
      {
        id: nextId,
        kind: 'system',
        title: formatSystemInfoTabTitle(activeTab.title),
        sessionTabId: activeTab.id,
        sourceTabTitle: activeTab.title
      }
    ])
    setTabOrder((prev) => insertTabKeyAfter(prev, homeTabKey(nextId), activeOrderKey))
    setActiveLocalTabId(nextId)
    setError(null)
  }

  const handleCloseHomeTab = (event: MouseEvent<HTMLButtonElement>, homeTabId: string) => {
    event.stopPropagation()

    setLocalTabs((prev) => {
      const remaining = prev.filter((tab) => tab.id !== homeTabId)

      if (remaining.length === 0 && workspace.tabs.length === 0) {
        setActiveLocalTabId('home-1')
        setNextHomeTabNumber(2)
        setTabOrder((prevOrder) => {
          const filtered = prevOrder.filter((key) => key !== homeTabKey(homeTabId))
          return filtered.includes('home:home-1') ? filtered : ['home:home-1', ...filtered]
        })
        return [{ id: 'home-1', kind: 'home', title: t.untitledTab }]
      }

      if (activeLocalTabId === homeTabId) {
        setActiveLocalTabId(remaining.at(-1)?.id ?? null)
      }

      setTabOrder((prevOrder) => prevOrder.filter((key) => key !== homeTabKey(homeTabId)))
      return remaining
    })
  }

  const closeHomeTabs = (
    homeTabIds: string[],
    preferredActiveHomeId: string | null,
    nextSessionTabs: WorkspaceTab[]
  ) => {
    let nextHomeTabs = localTabs.filter((tab) => !homeTabIds.includes(tab.id))
    let nextOrder = tabOrder.filter((key) => {
      if (key.startsWith('home:')) {
        return nextHomeTabs.some((tab) => homeTabKey(tab.id) === key)
      }
      return nextSessionTabs.some((tab) => sessionTabKey(tab.id) === key)
    })

    if (!nextHomeTabs.length && !nextSessionTabs.length) {
      nextHomeTabs = [{ id: 'home-1', kind: 'home', title: t.untitledTab }]
      preferredActiveHomeId = 'home-1'
      nextOrder = nextOrder.includes('home:home-1') ? nextOrder : ['home:home-1', ...nextOrder]
      setNextHomeTabNumber((prev) => Math.max(prev, 2))
    } else if (preferredActiveHomeId && !nextHomeTabs.some((tab) => tab.id === preferredActiveHomeId)) {
      preferredActiveHomeId = nextHomeTabs.at(-1)?.id ?? null
    }

    setLocalTabs(nextHomeTabs)
    setActiveLocalTabId(preferredActiveHomeId)
    setTabOrder(nextOrder)
  }

  const closeSessionTabs = async (tabIds: string[]) => {
    if (!desktopApi || !tabIds.length) {
      return
    }

    let lastSnapshot: WorkspaceSnapshot | null = null
    for (const tabId of tabIds) {
      lastSnapshot = await desktopApi.closeTab(tabId)
    }

    if (lastSnapshot) {
      applySnapshot(lastSnapshot)
    }
  }

  const handleTabContextAction = async (
    action: 'copy' | 'connect' | 'connectAll' | 'disconnect' | 'close' | 'closeOthers' | 'closeAll'
  ) => {
    if (!tabContextMenu) {
      return
    }

    const target = tabContextMenu.target
    setTabContextMenu(null)

    if (action === 'copy') {
      navigator.clipboard?.writeText?.(target.title)
      return
    }

    if (action === 'connect') {
      if (target.kind !== 'session' || !desktopApi) {
        return
      }
      try {
        setIsBusy(true)
        const snapshot = await desktopApi.reconnectTab(target.id)
        applySnapshot(snapshot)
        setActiveLocalTabId(null)
      } catch (err) {
        reportError(setError, '重新连接标签页', err)
      } finally {
        setIsBusy(false)
      }
      return
    }

    if (action === 'connectAll') {
      if (!desktopApi) {
        return
      }
      const reconnectableTabs = workspace.tabs.filter((tab) => tab.status !== 'connected' && tab.status !== 'connecting')
      if (!reconnectableTabs.length) {
        return
      }
      try {
        setIsBusy(true)
        let lastSnapshot: WorkspaceSnapshot | null = null
        for (const tab of reconnectableTabs) {
          lastSnapshot = await desktopApi.reconnectTab(tab.id)
        }
        if (lastSnapshot) {
          applySnapshot(lastSnapshot)
          setActiveLocalTabId(null)
        }
      } catch (err) {
        reportError(setError, '连接全部 SSH', err)
      } finally {
        setIsBusy(false)
      }
      return
    }

    if (action === 'disconnect') {
      if (target.kind !== 'session' || !desktopApi) {
        return
      }
      try {
        setIsBusy(true)
        const snapshot = await desktopApi.disconnectTab(target.id)
        applySnapshot(snapshot)
      } catch (err) {
        reportError(setError, '断开标签页', err)
      } finally {
        setIsBusy(false)
      }
      return
    }

    const sessionTabsToClose = action === 'closeAll'
      ? workspace.tabs.map((tab) => tab.id)
      : action === 'close'
        ? target.kind === 'session' ? [target.id] : []
        : target.kind === 'session'
          ? workspace.tabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
          : workspace.tabs.map((tab) => tab.id)

    const homeTabsToClose = action === 'closeAll'
      ? localTabs.map((tab) => tab.id)
      : action === 'close'
        ? target.kind === 'local' ? [target.id] : []
        : target.kind === 'local'
          ? localTabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
          : localTabs.map((tab) => tab.id)

    const remainingSessionTabs = workspace.tabs.filter((tab) => !sessionTabsToClose.includes(tab.id))
    const preferredActiveHomeId = target.kind === 'local' && action !== 'close' ? target.id : null
    closeHomeTabs(homeTabsToClose, preferredActiveHomeId, remainingSessionTabs)

    if (!sessionTabsToClose.length) {
      return
    }

    try {
      setIsBusy(true)
      await closeSessionTabs(sessionTabsToClose)
      if (!remainingSessionTabs.length) {
        setActiveLocalTabId((prev) => prev ?? preferredActiveHomeId ?? localTabsRef.current.at(-1)?.id ?? 'home-1')
      }
    } catch (err) {
      reportError(setError, '关闭标签组', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleDropUpload = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    const localPaths = extractDroppedLocalPaths(event)

    if (!localPaths.length || !desktopApi || !activeTab || !activeSession) {
      setError(t.desktopOnlyUpload)
      return
    }

    try {
      setIsBusy(true)
      await uploadLocalPaths(localPaths)
    } catch (err) {
      reportError(setError, '上传文件', err)
    } finally {
      setIsBusy(false)
    }
  }

  const openLocalDirectory = async (targetPath: string) => {
    if (!desktopApi) {
      setLocalPath(targetPath)
      return
    }

    const { path, items } = await desktopApi.listLocalDirectory(targetPath)
    setLocalPath(path)
    setLocalItems(withParentRow(path, items))
  }

  const handleOpenLocalItem = async (item: LocalFileItem) => {
    if (!desktopApi) {
      setLocalPath(item.path)
      return
    }

    try {
      if (item.type === 'folder') {
        await openLocalDirectory(item.path)
        return
      }
      await desktopApi.openFileEditorWindow({
        source: 'local',
        path: item.path,
        name: item.name,
        encoding: 'utf-8'
      })
    } catch (err) {
      reportError(setError, item.type === 'folder' ? '打开本地文件夹' : '打开本地文件', err)
    }
  }

  const openRemoteDirectory = async (tabId: string, targetPath: string, item?: RemoteFileItem) => {
    if (!desktopApi) {
      return
    }

    try {
      const snapshot = await desktopApi.openRemotePath(tabId, targetPath)
      applySnapshot(snapshot)
    } catch (err) {
      throw new Error(formatAppError('打开远程目录', err, { targetPath, item }))
    }
  }

  const openRemoteFileForEdit = async (tabId: string, item: RemoteFileItem) => {
    if (!desktopApi) {
      return
    }
    await desktopApi.openFileEditorWindow({
      source: 'remote',
      path: item.path,
      name: item.name,
      tabId,
      encoding: 'utf-8'
    })
  }

  const handleSaveFileEditor = async (content: string, encoding: string) => {
    if (!desktopApi || !fileEditor) {
      return
    }

    try {
      setIsBusy(true)
      if (fileEditor.source === 'local') {
        await desktopApi.writeLocalFile(fileEditor.path, content, encoding)
        if (!isFileEditorWindow) {
          await openLocalDirectory(localPath)
        }
      } else if (fileEditor.tabId ?? activeTab?.id) {
        const snapshot = await desktopApi.writeRemoteFile(fileEditor.tabId ?? activeTab!.id, fileEditor.path, content, encoding)
        applySnapshot(snapshot)
      }
      setFileEditor((prev) => prev ? { ...prev, content, encoding } : prev)
      setFileEditorError(null)
    } catch (err) {
      reportError(setFileEditorError, '保存文件', err, { targetPath: fileEditor.path })
    } finally {
      setIsBusy(false)
    }
  }

  const handleReloadFileEditorWithEncoding = async (encoding: string) => {
    if (!desktopApi || !fileEditor) {
      return
    }

    try {
      setIsBusy(true)
      const content = fileEditor.source === 'local'
        ? await desktopApi.readLocalFile(fileEditor.path, encoding)
        : (fileEditor.tabId ?? activeTab?.id)
          ? await desktopApi.readRemoteFile(fileEditor.tabId ?? activeTab!.id, fileEditor.path, encoding)
          : fileEditor.content
      setFileEditor({ ...fileEditor, content, encoding })
      setFileEditorError(null)
    } catch (err) {
      reportError(setFileEditorError, '重新按编码打开文件', err)
    } finally {
      setIsBusy(false)
    }
  }

  const refreshCurrentPane = async (pane: 'local' | 'remote') => {
    if (pane === 'local') {
      await openLocalDirectory(localPath)
      return
    }

    if (activeTab && activeSession) {
      await openRemoteDirectory(activeTab.id, activeSession.remotePath)
    }
  }

  const setClipboardItems = (operation: 'copy' | 'cut', pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>) => {
    const normalizedItems = items
      .filter((item) => item.name !== '..')
      .map((item) => ({
        pane,
        path: item.path,
        name: item.name,
        type: item.type
      }))

    if (!normalizedItems.length) {
      return
    }

    const nextClipboard = {
      pane,
      operation,
      items: normalizedItems,
      tabId: pane === 'remote' ? activeTab?.id : undefined
    } satisfies FileClipboardState

    setFileClipboard((current) => {
      if (
        current
        && current.pane === nextClipboard.pane
        && current.operation === nextClipboard.operation
        && current.tabId === nextClipboard.tabId
        && areClipboardItemsEqual(current.items, nextClipboard.items)
      ) {
        return null
      }

      return nextClipboard
    })
  }

  const canPasteIntoLocal = Boolean(fileClipboard)

  const canPasteIntoRemote = Boolean(
    fileClipboard
    && activeTab
    && (
      fileClipboard.pane !== 'remote'
      || fileClipboard.tabId === activeTab.id
    )
  )

  const localCutPaths = fileClipboard?.operation === 'cut' && fileClipboard.pane === 'local'
    ? fileClipboard.items.map((item) => item.path)
    : []
  const remoteCutPaths = fileClipboard?.operation === 'cut' && fileClipboard.pane === 'remote'
    ? fileClipboard.items.map((item) => item.path)
    : []
  const clipboardStatusText = fileClipboard
    ? fileClipboard.operation === 'cut'
      ? (locale === 'zhCN'
        ? `已剪切 ${fileClipboard.items.length} 个文件，按 Esc 取消`
        : `Cut ${fileClipboard.items.length} files, press Esc to cancel`)
      : (locale === 'zhCN'
        ? `已复制 ${fileClipboard.items.length} 个文件，可在其他目录粘贴，按 Esc 取消`
        : `Copied ${fileClipboard.items.length} files, ready to paste in another folder. Press Esc to cancel`)
    : null

  const clearCutState = () => {
    setFileClipboard(null)
  }

  const handlePasteIntoPane = (pane: 'local' | 'remote') => {
    if (!desktopApi || !fileClipboard) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)

        const destinationDirectory = pane === 'local' ? localPath : activeSession?.remotePath
        if (!destinationDirectory) {
          return
        }

        if (pane === 'remote' && !activeTab) {
          return
        }

        if (fileClipboard.pane === 'remote' && pane === 'remote' && fileClipboard.tabId !== activeTab?.id) {
          throw new Error('暂不支持跨远程会话粘贴，请在原会话内操作或先下载到本地')
        }

        const existingNames = pane === 'local'
          ? localItems.filter((item) => item.name !== '..').map((item) => item.name)
          : (activeSession?.remoteFiles ?? []).filter((item) => item.name !== '..').map((item) => item.name)
        const targetNames = allocateTargetNames(fileClipboard.items, existingNames, fileClipboard.operation, destinationDirectory)

        if (fileClipboard.pane === 'local' && pane === 'local') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const destinationPath = joinLocalPath(destinationDirectory, targetNames[index]!)
            if (fileClipboard.operation === 'copy') {
              await desktopApi.copyLocalPath(item.path, destinationPath)
            } else {
              await desktopApi.moveLocalPath(item.path, destinationPath)
            }
          }
          await openLocalDirectory(localPath)
        } else if (fileClipboard.pane === 'local' && pane === 'remote') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const snapshot = await desktopApi.uploadFile(activeTab!.id, item.path, destinationDirectory, {
              targetName: targetNames[index]
            })
            applySnapshot(snapshot)
            if (fileClipboard.operation === 'cut') {
              await desktopApi.deleteLocalPath(item.path)
            }
          }
          await openLocalDirectory(localPath)
          await refreshCurrentPane('remote')
        } else if (fileClipboard.pane === 'remote' && pane === 'local') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const snapshot = await desktopApi.downloadRemotePath(fileClipboard.tabId!, item.path, item.type, destinationDirectory, {
              targetName: targetNames[index]
            })
            applySnapshot(snapshot)
            if (fileClipboard.operation === 'cut') {
              const deleteSnapshot = await desktopApi.deleteRemotePath(fileClipboard.tabId!, item.path, item.type)
              applySnapshot(deleteSnapshot)
            }
          }
          await openLocalDirectory(localPath)
          if (fileClipboard.tabId === activeTab?.id) {
            await refreshCurrentPane('remote')
          }
        } else if (fileClipboard.pane === 'remote' && pane === 'remote') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const destinationPath = joinRemotePath(destinationDirectory, targetNames[index]!)
            const snapshot = fileClipboard.operation === 'copy'
              ? await desktopApi.copyRemotePath(activeTab!.id, item.path, destinationPath, item.type)
              : await desktopApi.moveRemotePath(activeTab!.id, item.path, destinationPath)
            applySnapshot(snapshot)
          }
          await refreshCurrentPane('remote')
        }

        if (fileClipboard.operation === 'cut') {
          setFileClipboard(null)
        }
      } catch (err) {
        reportError(setError, '粘贴文件', err)
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const runFileAction = async (action: () => Promise<void>) => {
    try {
      setIsBusy(true)
      setIsFileActionSubmitting(true)
      await action()
      setFileActionDialog(null)
      setFileActionError(null)
    } catch (err) {
      reportError(setFileActionError, '文件操作', err)
    } finally {
      setIsFileActionSubmitting(false)
      setIsBusy(false)
    }
  }

  const handleSubmitFileAction = async (rawValue: string) => {
    if (!desktopApi || !fileActionDialog) {
      return
    }

    const value = rawValue.trim()

    if (fileActionDialog.kind === 'delete') {
      await runFileAction(async () => {
        const [firstTarget] = fileActionDialog.targets
        if (!firstTarget) {
          return
        }
        if (firstTarget.pane === 'local') {
          for (const target of fileActionDialog.targets) {
            await desktopApi.deleteLocalPath(target.path)
          }
        } else if (activeTab) {
          for (const target of fileActionDialog.targets) {
            const snapshot = await desktopApi.deleteRemotePath(activeTab.id, target.path, target.type)
            applySnapshot(snapshot)
          }
        }
        await refreshCurrentPane(firstTarget.pane)
      })
      return
    }

    if (!value) {
      setFileActionError(t.fileNameRequired)
      return
    }

    if (fileActionDialog.kind === 'new-folder') {
      await runFileAction(async () => {
        if (fileActionDialog.pane === 'local') {
          await desktopApi.createLocalDirectory(fileActionDialog.directoryPath, value)
        } else if (activeTab) {
          const snapshot = await desktopApi.createRemoteDirectory(activeTab.id, fileActionDialog.directoryPath, value)
          applySnapshot(snapshot)
        }
        await refreshCurrentPane(fileActionDialog.pane)
      })
      return
    }

    if (fileActionDialog.kind === 'new-file') {
      await runFileAction(async () => {
        if (fileActionDialog.pane === 'local') {
          await desktopApi.createLocalFile(fileActionDialog.directoryPath, value)
        } else if (activeTab) {
          const snapshot = await desktopApi.createRemoteFile(activeTab.id, fileActionDialog.directoryPath, value)
          applySnapshot(snapshot)
        }
        await refreshCurrentPane(fileActionDialog.pane)
      })
      return
    }

    if (fileActionDialog.kind === 'rename') {
      await runFileAction(async () => {
        if (fileActionDialog.target.pane === 'local') {
          await desktopApi.renameLocalPath(fileActionDialog.target.path, value)
        } else if (activeTab) {
          const snapshot = await desktopApi.renameRemotePath(activeTab.id, fileActionDialog.target.path, value)
          applySnapshot(snapshot)
        }
        await refreshCurrentPane(fileActionDialog.target.pane)
      })
      return
    }

  }

  const requestNewFolder = (pane: 'local' | 'remote', directoryPath: string) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-folder', pane, directoryPath })
  }

  const requestNewFile = (pane: 'local' | 'remote', directoryPath: string) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-file', pane, directoryPath })
  }

  const requestRename = (pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'rename',
      target: { pane, path: item.path, name: item.name, type: item.type }
    })
  }

  const requestDelete = (pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'delete',
      targets: items.map((item) => ({ pane, path: item.path, name: item.name, type: item.type }))
    })
  }

  const requestChangePermissions = (pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem) => {
    setPermissionDialogError(null)
    setPermissionDialog({
      target: { pane, path: item.path, name: item.name, type: item.type, permission: item.permission },
      supportsRecursive: item.type === 'folder' && (pane === 'local' || activeTab?.sessionType === 'ssh')
    })
  }

  const handleSubmitPermissions = async (options: PermissionChangeOptions) => {
    if (!desktopApi || !permissionDialog) {
      return
    }

    try {
      setIsBusy(true)
      const { target } = permissionDialog
      if (target.pane === 'local') {
        await desktopApi.changeLocalPermissions(target.path, options)
      } else if (activeTab) {
        const snapshot = await desktopApi.changeRemotePermissions(activeTab.id, target.path, options)
        applySnapshot(snapshot)
      }
      await refreshCurrentPane(target.pane)
      setPermissionDialog(null)
      setPermissionDialogError(null)
    } catch (err) {
      reportError(setPermissionDialogError, '修改文件权限', err)
    } finally {
      setIsBusy(false)
    }
  }

  const handleQuickDelete = (pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>) => {
    if (!desktopApi || pane !== 'remote' || !activeTab || !items.length) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        for (const item of items) {
          const snapshot = await desktopApi.deleteRemotePath(activeTab.id, item.path, item.type)
          applySnapshot(snapshot)
        }
        await refreshCurrentPane('remote')
      } catch (err) {
        const firstItem = items[0]
        reportError(setError, '快速删除远程文件', err, firstItem ? { item: firstItem, targetPath: firstItem.path } : undefined)
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const uploadLocalPaths = async (paths: string[]) => {
    if (!desktopApi || !activeTab || !activeSession) {
      return
    }

    const uniquePaths = Array.from(new Set(paths))
    if (uniquePaths.length > 1) {
      const snapshot = await desktopApi.queueUpload(uniquePaths.map(fileNameFromPath))
      applySnapshot(snapshot)
    }

    for (const localPath of uniquePaths) {
      const snapshot = await desktopApi.uploadFile(activeTab.id, localPath, activeSession.remotePath)
      applySnapshot(snapshot)
    }
  }

  const orderedTabs: OrderedTabEntry[] = tabOrder
    .map((key) => {
      if (key.startsWith('home:')) {
        const id = key.slice(5)
        const localTab = localTabs.find((tab) => tab.id === id)
        return localTab
          ? {
              key,
              kind: 'local' as const,
              id: localTab.id,
              title: localTab.title,
              tabKind: localTab.kind
            }
          : null
      }

      const id = key.slice(8)
      const sessionTab = workspace.tabs.find((tab) => tab.id === id)
      return sessionTab ? { key, kind: 'session' as const, tab: sessionTab } : null
    })
    .filter((item): item is OrderedTabEntry => item !== null)

  const handleOpenRemoteItem = (item: RemoteFileItem) => {
    if (!desktopApi || !activeTab) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        if (item.type === 'folder') {
          await openRemoteDirectory(activeTab.id, item.path, item)
        } else {
          await openRemoteFileForEdit(activeTab.id, item)
        }
      } catch (err) {
        reportError(setError, item.type === 'folder' ? '打开远程文件夹' : '打开远程文件', err, { targetPath: item.path, item })
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleOpenRemotePath = (targetPath: string) => {
    if (!activeTab) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        await openRemoteDirectory(activeTab.id, targetPath)
      } catch (err) {
        reportError(setError, '打开远程路径', err, { targetPath })
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleRefreshWorkspace = () => {
    if (!activeTab || !activeSession) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        setFileClipboard(null)
        await openLocalDirectory(localPath)
        await openRemoteDirectory(activeTab.id, activeSession.remotePath)
      } catch (err) {
        reportError(setError, '刷新工作区', err, { targetPath: activeSession.remotePath })
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleToggleRemoteFileAccessMode = () => {
    if (!desktopApi || !activeTab || activeTab.sessionType !== 'ssh' || !activeSession) {
      return
    }

    const nextMode = activeSession.fileAccessMode === 'root' ? 'user' : 'root'
    if (nextMode === 'root') {
      if (!activeSession.hasReusableSudoAuth) {
        setRootAccessDialogError(null)
        setRootAccessDialog({
          tabId: activeTab.id,
          sshUser: activeProfile?.type === 'ssh' ? activeProfile.username : undefined,
          sudoUser: activeSession.sudoUser || 'root'
        })
        return
      }

      void (async () => {
        try {
          setIsBusy(true)
          setRootAccessDialogError(null)
          const snapshot = await desktopApi.setRemoteFileAccessMode(activeTab.id, nextMode)
          applySnapshot(snapshot)
        } catch (err) {
          if (shouldPromptForRootAccess(err)) {
            setRootAccessDialog({
              tabId: activeTab.id,
              sshUser: activeProfile?.type === 'ssh' ? activeProfile.username : undefined,
              sudoUser: activeSession.sudoUser || 'root'
            })
            setRootAccessDialogError(normalizeErrorMessage(err))
            return
          }
          reportError(setError, '切换到 root 视角', err)
        } finally {
          setIsBusy(false)
        }
      })()
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        const snapshot = await desktopApi.setRemoteFileAccessMode(activeTab.id, nextMode)
        applySnapshot(snapshot)
      } catch (err) {
        reportError(setError, '切换到普通视角', err)
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleConfirmRootAccess = ({ sudoUser, sudoPassword }: { sudoUser: string; sudoPassword: string }) => {
    if (!desktopApi || !rootAccessDialog) {
      return
    }

    void (async () => {
      try {
        setIsRootAccessSubmitting(true)
        setRootAccessDialogError(null)
        const snapshot = await desktopApi.setRemoteFileAccessMode(rootAccessDialog.tabId, 'root', {
          sudoUser,
          sudoPassword
        })
        applySnapshot(snapshot)
        setRootAccessDialog(null)
        setRootAccessDialogError(null)
      } catch (err) {
        reportError(setRootAccessDialogError, '切换到 root 视角', err)
      } finally {
        setIsRootAccessSubmitting(false)
      }
    })()
  }

  const handleUploadFiles = (items: LocalFileItem[]) => {
    if (!desktopApi) {
      return
    }

    void (async () => {
      try {
        setIsBusy(true)
        await uploadLocalPaths(items.map((item) => item.path))
      } catch (err) {
        reportError(setError, '上传文件', err)
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleChooseUploadFiles = () => {
    if (!desktopApi) {
      return
    }

    void (async () => {
      const filePaths = await desktopApi.selectLocalFiles(localPath)
      if (!filePaths.length) {
        return
      }

      try {
        setIsBusy(true)
        await uploadLocalPaths(filePaths)
      } catch (err) {
        reportError(setError, '上传文件', err)
      } finally {
        setIsBusy(false)
      }
    })()
  }

  const handleDownloadFiles = (items: RemoteFileItem[], targetDirectory?: string) => {
    if (!desktopApi || !activeTab) {
      return
    }

    void (async () => {
      const files = items.filter((row) => row.type === 'file')
      if (!files.length) {
        return
      }

      const downloadDirectory = targetDirectory ?? await desktopApi.selectLocalDirectory()
      if (!downloadDirectory) {
        return
      }

      try {
        setIsBusy(true)
        for (const item of files) {
          const snapshot = await desktopApi.downloadFile(activeTab.id, item.path, downloadDirectory)
          applySnapshot(snapshot)
        }
        await openLocalDirectory(downloadDirectory)
      } catch (err) {
        reportError(setError, '下载文件', err, { targetPath: downloadDirectory })
      } finally {
        setIsBusy(false)
      }
    })()
  }

  if (isConnectionManagerWindow) {
    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={t.connectionManager} />
        <ConnectionManagerModal
          profiles={workspace.profiles}
          folders={workspace.folders || []}
          standalone
          onClose={closeCurrentWindow}
          onCreate={openCreateConnection}
          onDeleteProfile={handleDeleteProfile}
          onEditProfile={openEditConnection}
              onOpenProfile={(profileId) => {
                if (desktopApi) {
                  void desktopApi.openProfileFromManager(profileId).then(() => {
                    closeCurrentWindow()
                  }).catch((err: Error) => {
                    reportError(setError, '从管理器打开连接', err)
                  })
                  return
                }
                void handleOpenProfile(profileId)
              }}
          onCreateFolder={(name) => desktopApi?.createFolder(name)}
          onDeleteFolder={(id) => desktopApi?.deleteFolder(id)}
          onUpdateFolder={(id, updates) => desktopApi?.updateFolder(id, updates)}
          onUpdateOrder={(id, parentId, order) => desktopApi?.updateEntityOrder(id, parentId, order)}
        />
        {showForm ? (
          <ConnectionModal
            errorMessage={formError}
            groupOptions={connectionGroupOptions}
            mode={editingProfileId ? 'edit' : 'create'}
            form={form}
            setForm={updateForm}
            onClearHostFingerprint={() => {
              const editingProfile = editingProfileId
                ? workspace.profiles.find((profile) => profile.id === editingProfileId) ?? null
                : null
              if (editingProfile) {
                void handleClearHostFingerprint(editingProfile)
                setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
              }
            }}
            onSubmit={handleSaveProfile}
            onClose={() => {
              setShowForm(false)
              setEditingProfileId(null)
              setFormError(null)
            }}
          />
        ) : null}
      </>
    )
  }

  if (isCommandManagerWindow) {
    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={t.commandManager} />
        <CommandManagerModal
          commandFolders={workspace.commandFolders || []}
          commandTemplates={workspace.commandTemplates || []}
          standalone
          onClose={closeCurrentWindow}
          onCreateFolder={(name) => {
            void createCommandFolder(name)
          }}
          onDeleteFolder={(folderId) => {
            void deleteCommandFolder(folderId)
          }}
          onUpdateFolder={(folderId, updates) => {
            void updateCommandFolder(folderId, updates)
          }}
          onUpdateOrder={(id, parentId, order) => {
            void updateCommandOrder(id, parentId, order)
          }}
          onCreateCommand={(input) => {
            void saveCommandTemplate(null, input)
          }}
          onUpdateCommand={(commandId, input) => {
            void saveCommandTemplate(commandId, input)
          }}
          onDeleteCommand={(commandId) => {
            void deleteCommandTemplate(commandId)
          }}
        />
      </>
    )
  }

  if (isCommandFormWindow) {
    const editingCommand = formWindowMode === 'edit'
      ? workspace.commandTemplates.find((item) => item.id === formWindowCommandId) ?? null
      : null

    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={editingCommand ? t.commandEdit : t.commandCreate} />
        <CommandEditorModal
          folders={workspace.commandFolders || []}
          initialValue={editingCommand
            ? toCommandTemplateInput(editingCommand)
            : {
                ...emptyCommandForm,
                parentId: formWindowFolderId || undefined
              }}
          mode={editingCommand ? 'edit' : formWindowMode}
          standalone
          onClose={closeCurrentWindow}
          onSubmit={(input) => {
            void saveCommandTemplate(editingCommand?.id ?? null, input)
          }}
        />
      </>
    )
  }

  if (isConnectionFormWindow) {
    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={editingProfileId ? t.editConnection : t.newConnection} />
        <ConnectionModal
          errorMessage={formError}
          groupOptions={connectionGroupOptions}
          mode={editingProfileId ? 'edit' : formWindowMode}
          form={form}
          setForm={updateForm}
          onClearHostFingerprint={() => {
            const editingProfile = editingProfileId
              ? workspace.profiles.find((profile) => profile.id === editingProfileId) ?? null
              : null
            if (editingProfile) {
              void handleClearHostFingerprint(editingProfile)
              setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
            }
          }}
          standalone
          onSubmit={handleSaveProfile}
          onClose={closeCurrentWindow}
        />
      </>
    )
  }

  if (isFileEditorWindow && fileEditor) {
    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={fileEditor.name} />
        <FileEditorModal
          errorMessage={fileEditorError}
          file={fileEditor}
          isBusy={isBusy}
          onClose={closeCurrentWindow}
          onReloadWithEncoding={(encoding) => {
            void handleReloadFileEditorWithEncoding(encoding)
          }}
          onSave={handleSaveFileEditor}
          standalone
          themeMode={themeMode}
        />
      </>
    )
  }

  if (isFileEditorWindow) {
    return (
      <>
        <StandaloneWindowTitlebar isWindows={isWindowsDesktop} title={fileEditorWindowName ?? t.appTitle} />
        <div className="standalone-shell file-editor-window">
          <div className={`modal-card file-editor-modal ${themeMode === 'default-dark' ? 'file-editor-modal--dark' : ''} standalone`}>
            <div className="modal-header">
              <div className="file-editor-title">
                <span>{fileEditorWindowSource === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
                <strong>{fileEditorWindowName ?? ''}</strong>
              </div>
            </div>
            {fileEditorError ? <div className="modal-error">{fileEditorError}</div> : <div className="file-editor-path">{t.updating}</div>}
          </div>
        </div>
      </>
    )
  }

  return (
    <>
      <div className={`fs-shell ${isWindowsDesktop ? 'has-window-menubar' : ''}`} style={{ '--sidebar-width': `${sidebarWidth}px` } as CSSProperties}>
        {isWindowsDesktop ? (
          <div className="window-menubar">
            <div className="window-brandmark" aria-label={t.appTitle}>
              <AppIcon name="brand" size={18} />
              <strong>{t.appTitle}</strong>
            </div>
            <div className="window-control-buttons">
              <button aria-label="Minimize" type="button" onClick={() => { void desktopApi?.minimizeCurrentWindow() }}>−</button>
              <button aria-label="Maximize" type="button" onClick={() => { void desktopApi?.toggleMaximizeCurrentWindow() }}>□</button>
              <button aria-label="Close" className="window-close-button" type="button" onClick={() => { void desktopApi?.closeCurrentWindow() }}>×</button>
            </div>
          </div>
        ) : null}
        <TabBar
          activeHomeTabId={activeLocalTabId}
          activeSessionTabId={workspace.activeTabId}
          locale={locale}
          onAddHomeTab={handleAddHomeTab}
          onActivateHome={handleActivateHome}
          onActivateSession={(tabId) => {
            void handleActivateTab(tabId)
          }}
          onCloseHomeTab={handleCloseHomeTab}
          onCloseSessionTab={(event, tabId) => {
            void handleCloseTab(event, tabId)
          }}
          onDragEnd={() => setDraggingTabKey(null)}
          onDragEnter={(targetKey) => {
            setTabOrder((prev) => reorderTabKeys(prev, draggingTabKey, targetKey))
          }}
          onDragStart={setDraggingTabKey}
          onOpenCommandManager={openCommandManager}
          onOpenConnectionManager={() => {
            if (desktopApi) {
              void desktopApi.openConnectionManagerWindow()
              return
            }
            setShowConnectionManager(true)
          }}
          onOpenLogsDirectory={() => {
            if (!desktopApi) {
              setError(t.desktopOnlyOpenLogs)
              return
            }
            void desktopApi.openLogsDirectory().catch((err) => {
              reportError(setError, t.openLogsDirectory, err)
            })
          }}
          onOpenTabContext={(event, target) => {
            setTabContextMenu({ x: event.clientX, y: event.clientY, target })
          }}
          onSetLocale={(nextLocale) => {
            setLocale(nextLocale)
            setLocaleState(nextLocale)
          }}
          onSetTheme={setThemeMode}
          orderedTabs={orderedTabs}
          theme={themeMode}
        />

        {showSidebar ? (
          <aside className="fs-sidebar" style={{ position: 'relative' }}>
            <SystemSidebar activeProfile={activeProfile} activeSession={activeSession} onOpenSystemInfo={handleOpenSystemInfo} />
            <div
              aria-label={t.resizeSidebar}
              className={`sidebar-resizer ${isResizingSidebar ? 'is-active' : ''}`}
              onMouseDown={() => setIsResizingSidebar(true)}
              role="separator"
            />
          </aside>
        ) : null}

        <main className={`fs-main ${error ? 'has-status' : 'no-status'} ${showSidebar ? '' : 'full-width'}`}>
          {error ? (
            <div className="status-message" role="alert">
              <span className="status-message-text">{error}</span>
              <button
                aria-label={t.closeTab}
                className="status-message-close"
                onClick={() => setError(null)}
                type="button"
              >
                ×
              </button>
            </div>
          ) : null}
          <div className="workspace-stage">
            <WorkspaceStage
              activeLocalTab={activeLocalTab}
              activeProfile={activeProfile}
              activeSession={activeSession}
              activeTab={activeTab}
              tabs={workspace.tabs}
              commandFolders={workspace.commandFolders || []}
              commandTemplates={workspace.commandTemplates || []}
              folders={workspace.folders || []}
              isBusy={isBusy}
              localItems={localItems}
              localPath={localPath}
              canPasteToLocal={canPasteIntoLocal}
              canPasteToRemote={canPasteIntoRemote}
              clipboardStatusText={clipboardStatusText}
              localCutPaths={localCutPaths}
              remoteCutPaths={remoteCutPaths}
              onCopyItems={setClipboardItems.bind(null, 'copy')}
              onCutItems={setClipboardItems.bind(null, 'cut')}
              onClearCutState={clearCutState}
              onExecuteCommand={(commandId, args, options, scope) => {
                void executeCommandTemplate(commandId, args, options, scope)
              }}
              onOpenCommandManager={openCommandManager}
              profiles={workspace.profiles}
              onChooseUploadFiles={handleChooseUploadFiles}
              onDownloadFiles={handleDownloadFiles}
              onDropUpload={handleDropUpload}
              onOpenLocalItem={handleOpenLocalItem}
              onOpenLocalPath={(targetPath) => {
                void openLocalDirectory(targetPath).catch((err: Error) => reportError(setError, '打开本地路径', err, { targetPath }))
              }}
              onOpenProfile={handleOpenProfile}
              onOpenRemoteItem={handleOpenRemoteItem}
              onOpenRemotePath={handleOpenRemotePath}
              onPasteIntoPane={handlePasteIntoPane}
              onRequestChangePermissions={requestChangePermissions}
              onRequestDelete={requestDelete}
              onRequestNewFile={requestNewFile}
              onRequestNewFolder={requestNewFolder}
              onRequestQuickDelete={handleQuickDelete}
              onRequestRename={requestRename}
              onToggleRemoteFileAccessMode={handleToggleRemoteFileAccessMode}
              remoteFileAccessMode={activeSession?.fileAccessMode ?? 'user'}
              onRefresh={handleRefreshWorkspace}
              onUploadFiles={handleUploadFiles}
            />
          </div>
        </main>

        <TransferBar
          activeCount={activeTransferCount}
          fullWidth={!showSidebar}
          isPending={isBusy}
          onOpen={() => setShowTransfers((prev) => !prev)}
          transfers={workspace.transfers}
        />

        {showTransfers ? (
            <TransferPopover
              transfers={workspace.transfers}
              onCancelTransfer={(transferId) => {
                if (!desktopApi) return
                void desktopApi.cancelTransfer(transferId).then((snapshot) => {
                  applySnapshot(snapshot)
                }).catch((err: Error) => {
                  reportError(setError, '取消传输', err)
                })
              }}
              onClearTransfers={(transferIds) => {
                if (!desktopApi || !transferIds.length) return
                void desktopApi.clearTransfers(transferIds).then((snapshot) => {
                  applySnapshot(snapshot)
                }).catch((err: Error) => {
                  reportError(setError, '清理传输记录', err)
                })
              }}
              onClose={() => setShowTransfers(false)}
            />
        ) : null}

        {rootAccessDialog ? (
          <RootAccessModal
            defaultSshUser={rootAccessDialog.sshUser}
            defaultSudoUser={rootAccessDialog.sudoUser}
            errorMessage={rootAccessDialogError}
            isSubmitting={isRootAccessSubmitting}
            onClose={() => {
              setRootAccessDialog(null)
              setRootAccessDialogError(null)
            }}
            onSubmit={handleConfirmRootAccess}
          />
        ) : null}

        {sshInteraction?.kind === 'credentials' ? (
          <SshCredentialsModal
            errorMessage={sshInteractionError}
            request={sshInteraction}
            onCancel={() => {
              void resolveSshInteraction(sshInteraction.requestId, {
                kind: 'credentials',
                canceled: true
              })
            }}
            onSubmit={(input) => handleSubmitSshCredentials(sshInteraction, input)}
          />
        ) : null}

        {sshInteraction?.kind === 'host-verification' ? (
          <SshHostVerificationModal
            request={sshInteraction}
            onReject={() => {
              void resolveSshInteraction(sshInteraction.requestId, {
                kind: 'host-verification',
                decision: 'cancel'
              })
            }}
            onAcceptOnce={() => {
              void resolveSshInteraction(sshInteraction.requestId, {
                kind: 'host-verification',
                decision: 'accept-once'
              })
            }}
            onAcceptAndSave={() => {
              void resolveSshInteraction(sshInteraction.requestId, {
                kind: 'host-verification',
                decision: 'accept-and-save'
              })
            }}
          />
        ) : null}
      </div>

      {tabContextMenu ? (
        <TabContextMenu
          canConnectAll={workspace.tabs.some((tab) => tab.status !== 'connected' && tab.status !== 'connecting')}
          canCloseAll={localTabs.length + workspace.tabs.length > 0}
          canCloseCurrent={tabContextMenu.target.kind === 'session' ? true : localTabs.length + workspace.tabs.length > 1}
          canCloseOthers={localTabs.length + workspace.tabs.length > 1}
          isSessionTab={tabContextMenu.target.kind === 'session'}
          onAction={(action) => {
            void handleTabContextAction(action)
          }}
          onClose={() => setTabContextMenu(null)}
          position={{ x: tabContextMenu.x, y: tabContextMenu.y }}
          tabStatus={tabContextMenu.target.kind === 'session' ? tabContextMenu.target.status : null}
        />
      ) : null}

      {showConnectionManager ? (
        <ConnectionManagerModal
          profiles={workspace.profiles}
          folders={workspace.folders || []}
          onClose={() => setShowConnectionManager(false)}
          onCreate={() => {
            setShowConnectionManager(false)
            openCreateConnection()
          }}
          onDeleteProfile={handleDeleteProfile}
          onEditProfile={(profile) => {
            setShowConnectionManager(false)
            openEditConnection(profile)
          }}
          onOpenProfile={(profileId) => {
            setShowConnectionManager(false)
            void handleOpenProfile(profileId)
          }}
          onCreateFolder={(name) => desktopApi?.createFolder(name)}
          onDeleteFolder={(id) => desktopApi?.deleteFolder(id)}
          onUpdateFolder={(id, updates) => desktopApi?.updateFolder(id, updates)}
          onUpdateOrder={(id, parentId, order) => desktopApi?.updateEntityOrder(id, parentId, order)}
        />

      ) : null}

      {showCommandManager ? (
        <CommandManagerModal
          commandFolders={workspace.commandFolders || []}
          commandTemplates={workspace.commandTemplates || []}
          onClose={() => setShowCommandManager(false)}
          onCreateFolder={(name) => {
            void createCommandFolder(name)
          }}
          onDeleteFolder={(folderId) => {
            void deleteCommandFolder(folderId)
          }}
          onUpdateFolder={(folderId, updates) => {
            void updateCommandFolder(folderId, updates)
          }}
          onUpdateOrder={(id, parentId, order) => {
            void updateCommandOrder(id, parentId, order)
          }}
          onCreateCommand={(input) => {
            void saveCommandTemplate(null, input)
          }}
          onUpdateCommand={(commandId, input) => {
            void saveCommandTemplate(commandId, input)
          }}
          onDeleteCommand={(commandId) => {
            void deleteCommandTemplate(commandId)
          }}
        />
      ) : null}

      {showForm ? (
        <ConnectionModal
          errorMessage={formError}
          groupOptions={connectionGroupOptions}
          mode={editingProfileId ? 'edit' : 'create'}
          form={form}
          setForm={updateForm}
          onClearHostFingerprint={() => {
            const editingProfile = editingProfileId
              ? workspace.profiles.find((profile) => profile.id === editingProfileId) ?? null
              : null
            if (editingProfile) {
              void handleClearHostFingerprint(editingProfile)
              setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
            }
          }}
          onSubmit={handleSaveProfile}
          onClose={() => {
            setShowForm(false)
            setEditingProfileId(null)
            setFormError(null)
          }}
        />
      ) : null}

      {fileActionDialog ? (
        <FileActionModal
          confirmLabel={
            fileActionDialog.kind === 'delete' ? t.delete : t.confirm
          }
          danger={fileActionDialog.kind === 'delete'}
          description={
            fileActionDialog.kind === 'delete'
              ? fileActionDialog.targets.length > 1
                ? `${t.deleteConfirmPrefix}${fileActionDialog.targets.length} ${t.itemsSuffix}${t.deleteConfirmSuffix}`
                : `${t.deleteConfirmPrefix}${fileActionDialog.targets[0]?.name ?? ''}${t.deleteConfirmSuffix}`
              : undefined
          }
          errorMessage={fileActionError}
          hint={fileActionDialog.kind === 'new-file' ? t.newFileExtensionHint : undefined}
          initialValue={
            fileActionDialog.kind === 'rename' ? fileActionDialog.target.name : ''
          }
          isSubmitting={isFileActionSubmitting}
          inputLabel={fileActionDialog.kind === 'delete' ? undefined : t.fileName}
          inputPlaceholder={
            fileActionDialog.kind === 'new-folder' ? t.folderName : t.fileName
          }
          onClose={() => {
            setFileActionDialog(null)
            setFileActionError(null)
            setIsFileActionSubmitting(false)
          }}
          onConfirm={(value) => {
            void handleSubmitFileAction(value)
          }}
          title={
            fileActionDialog.kind === 'new-folder'
              ? t.newFolder
              : fileActionDialog.kind === 'new-file'
                ? t.newFile
                : fileActionDialog.kind === 'rename'
                  ? t.rename
                  : t.delete
          }
        />
      ) : null}

      {permissionDialog ? (
        <FilePermissionModal
          errorMessage={permissionDialogError}
          fileName={permissionDialog.target.name}
          initialPermission={permissionDialog.target.permission}
          onClose={() => {
            setPermissionDialog(null)
            setPermissionDialogError(null)
          }}
          onSubmit={(options) => {
            void handleSubmitPermissions(options)
          }}
          supportsRecursive={permissionDialog.supportsRecursive}
        />
      ) : null}

      {closeConfirmDialog ? (
        <div className="modal-backdrop">
          <div className="modal-card confirm-action-dialog">
            <div className="modal-header">
              <span>{t.closeConfirmTitle}</span>
              <button
                className="icon-button"
                onClick={() => {
                  setCloseConfirmDialog(null)
                  void desktopApi?.confirmCloseWindow('cancel')
                }}
                type="button"
              >
                ×
              </button>
            </div>
            <div className="confirm-action-dialog__description">
              {closeConfirmDialog.hasActiveConnections ? (
                <div style={{ color: 'var(--danger, #ef4444)', marginBottom: '12px', fontWeight: 'bold' }}>
                  {t.closeConfirmActiveWarn}
                </div>
              ) : closeConfirmDialog.isQuit ? (
                <div>{t.closeConfirmQuitMsg}</div>
              ) : null}
              {!closeConfirmDialog.isQuit ? (
                <div>{t.closeConfirmWindowsMsg}</div>
              ) : null}
            </div>
            <div className="form-actions confirm-action-dialog__actions" style={{ justifyContent: 'flex-end', gap: '8px' }}>
              <button
                className="flat-button"
                onClick={() => {
                  setCloseConfirmDialog(null)
                  void desktopApi?.confirmCloseWindow('cancel')
                }}
                type="button"
              >
                {t.cancel}
              </button>
              {!closeConfirmDialog.isQuit ? (
                <button
                  className="primary-button"
                  onClick={() => {
                    setCloseConfirmDialog(null)
                    void desktopApi?.confirmCloseWindow('hide')
                  }}
                  type="button"
                >
                  {t.closeConfirmHide}
                </button>
              ) : null}
              <button
                className="flat-button danger"
                onClick={() => {
                  setCloseConfirmDialog(null)
                  void desktopApi?.confirmCloseWindow('quit')
                }}
                type="button"
              >
                {t.closeConfirmQuit}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  )
}

function StandaloneWindowTitlebar({ isWindows, title }: { isWindows: boolean; title: string }) {
  const desktopApi = window.termdock
  if (!isWindows) {
    return null
  }

  return (
    <div className="standalone-window-titlebar">
      <div className="window-brandmark">
        <AppIcon name="brand" size={18} />
        <strong>{t.appTitle}</strong>
        <span>{title}</span>
      </div>
      <div className="window-control-buttons">
        <button aria-label="Minimize" type="button" onClick={() => { void desktopApi?.minimizeCurrentWindow() }}>−</button>
        <button aria-label="Maximize" type="button" onClick={() => { void desktopApi?.toggleMaximizeCurrentWindow() }}>□</button>
        <button aria-label="Close" className="window-close-button" type="button" onClick={() => { void desktopApi?.closeCurrentWindow() }}>×</button>
      </div>
    </div>
  )
}

function fileNameFromPath(filePath: string) {
  return filePath.split(/[/\\]/).pop() || filePath
}

function extractDroppedLocalPaths(event: DragEvent<HTMLDivElement>) {
  const desktopApi = window.termdock
  const fileList = Array.from(event.dataTransfer.files)
  const filePaths = (
    desktopApi?.getDroppedFilePaths?.(fileList)
    ?? fileList.map((file) => (file as File & { path?: string }).path).filter(Boolean)
  ).filter((filePath): filePath is string => Boolean(filePath))

  if (filePaths.length) {
    return filePaths
  }

  return Array.from(event.dataTransfer.items)
    .map((item) => item.getAsFile() as (File & { path?: string }) | null)
    .map((file) => file?.path)
    .filter((filePath): filePath is string => Boolean(filePath))
}
