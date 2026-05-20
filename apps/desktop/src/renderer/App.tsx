import { useEffect, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent } from 'react'
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
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { defaultForm, emptyState, localPreviewFiles, previewLocalPath, previewState, profileToForm } from './app/app-data'
import { homeTabKey, insertTabKeyAfter, isActiveTransfer, reorderTabKeys, sessionTabKey, withParentRow } from './app/app-utils'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from './features/commands/CommandEditorModal'
import { CommandManagerModal } from './features/commands/CommandManagerModal'
import { ConnectionManagerModal } from './features/connections/ConnectionManagerModal'
import { ConnectionModal } from './features/connections/ConnectionModal'
import { FileActionModal } from './features/files/FileActionModal'
import { FileEditorModal } from './features/files/FileEditorModal'
import { FilePermissionModal } from './features/files/FilePermissionModal'
import { RootAccessModal } from './features/files/RootAccessModal'
import { TabBar, type OrderedTabEntry, type TabContextTarget } from './features/layout/TabBar'
import { TabContextMenu } from './features/layout/TabContextMenu'
import { SystemSidebar } from './features/system/SystemSidebar'
import { TransferBar } from './features/transfers/TransferBar'
import { TransferPopover } from './features/transfers/TransferPopover'
import { WorkspaceStage } from './features/workspace/WorkspaceStage'
import { useThemeMode, type ThemeMode } from './hooks/useThemeMode'
import { defaultLocale, setLocale, t, type AppLocale } from './i18n'

const TERMINAL_TRANSCRIPT_LIMIT = 200_000
const STATUS_MESSAGE_TIMEOUT_MS = 15_000
const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i
const LOCALE_STORAGE_KEY = 'termdock.locale'

type ErrorDetails = {
  item?: RemoteFileItem
  targetPath?: string
}

type LocalTab =
  | { id: string; kind: 'home'; title: string }
  | { id: string; kind: 'system'; title: string; sessionTabId: string; sourceTabTitle: string }

function formatSystemInfoTabTitle(sourceTabTitle: string) {
  return `${t.systemInfoTabTitle} · ${sourceTabTitle || t.untitledTab}`
}

type FileDialogTarget = {
  pane: 'local' | 'remote'
  path: string
  name: string
  type: 'file' | 'folder'
}

type FileActionDialog =
  | { kind: 'new-folder'; pane: 'local' | 'remote'; directoryPath: string }
  | { kind: 'new-file'; pane: 'local' | 'remote'; directoryPath: string }
  | { kind: 'rename'; target: FileDialogTarget }
  | { kind: 'delete'; targets: FileDialogTarget[] }

function readStoredLocale(): AppLocale {
  if (typeof window === 'undefined') {
    return defaultLocale
  }

  const nextLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY)
  return nextLocale === 'enUS' || nextLocale === 'zhCN' ? nextLocale : defaultLocale
}

export function App() {
  const searchParams = new URLSearchParams(window.location.search)
  const windowMode = searchParams.get('window') ?? 'main'
  const isConnectionManagerWindow = windowMode === 'connection-manager'
  const isCommandManagerWindow = windowMode === 'command-manager'
  const isConnectionFormWindow = windowMode === 'connection-form'
  const isCommandFormWindow = windowMode === 'command-form'
  const isFileEditorWindow = windowMode === 'file-editor'
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
  const [showForm, setShowForm] = useState(false)
  const [showConnectionManager, setShowConnectionManager] = useState(false)
  const [showCommandManager, setShowCommandManager] = useState(false)
  const [form, setForm] = useState<CreateProfileInput>(defaultForm)
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null)
  const [localPath, setLocalPath] = useState(previewLocalPath)
  const [localItems, setLocalItems] = useState<LocalFileItem[]>(localPreviewFiles)
  const [localTabs, setLocalTabs] = useState<LocalTab[]>([{ id: 'home-1', kind: 'home', title: t.untitledTab }])
  const [activeLocalTabId, setActiveLocalTabId] = useState<string | null>('home-1')
  const [nextHomeTabNumber, setNextHomeTabNumber] = useState(2)
  const [tabOrder, setTabOrder] = useState<string[]>(['home:home-1'])
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
  const [rootAccessDialogError, setRootAccessDialogError] = useState<string | null>(null)
  const [showTransfers, setShowTransfers] = useState(false)
  const [themeMode, setThemeMode] = useState<ThemeMode>('default-dark')
  const [locale, setLocaleState] = useState<AppLocale>(() => readStoredLocale())

  useThemeMode(themeMode)

  const localTabsRef = useRef(localTabs)
  const previousActiveTransferCountRef = useRef(0)
  const pendingHomeReplacementKeyRef = useRef<string | null>(null)
  const desktopApi = window.termdock

  useEffect(() => {
    localTabsRef.current = localTabs
  }, [localTabs])

  useEffect(() => {
    setLocale(locale)
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale)
    setLocalTabs((prev) => prev.map((tab) => {
      if (tab.kind === 'home') {
        return { ...tab, title: t.untitledTab }
      }
      const sourceTabTitle = workspace.tabs.find((entry) => entry.id === tab.sessionTabId)?.title ?? tab.sourceTabTitle
      return {
        ...tab,
        sourceTabTitle,
        title: formatSystemInfoTabTitle(sourceTabTitle)
      }
    }))
  }, [locale, workspace.tabs])

  useEffect(() => {
    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
      setActiveLocalTabId(null)
      if (!isFileEditorWindow) {
        setTabOrder(['session:preview-tab-ssh'])
        setError(t.browserPreview)
      }
      return
    }

    desktopApi
      .getSnapshot()
      .then(setWorkspace)
      .catch((err: Error) => reportError(setError, '获取工作区快照', err))
  }, [desktopApi, isFileEditorWindow])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    const offTerminalData = desktopApi.onTerminalData(({ tabId, chunk }) => {
      setWorkspace((prev) => {
        const session = prev.sessions[tabId]
        if (!session) {
          return prev
        }

        return {
          ...prev,
          sessions: {
            ...prev.sessions,
            [tabId]: {
              ...session,
              terminalTranscript: appendTerminalTranscript(session.terminalTranscript, chunk)
            }
          }
        }
      })
    })

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
      offTerminalData()
      offSnapshot()
    }
  }, [desktopApi, isCommandFormWindow, isCommandManagerWindow, isConnectionFormWindow, isConnectionManagerWindow, isFileEditorWindow])

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
          return [...next, ...missing.slice(1)]
        }
      }

      return [...kept, ...missing]
    })
  }, [localTabs, workspace.tabs])

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

  const normalizeErrorMessage = (err: unknown) => {
    const rawMessage = err instanceof Error ? err.message : String(err)
    return rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()
  }

  const formatAppError = (scope: string, err: unknown, details?: ErrorDetails) => {
    const message = normalizeErrorMessage(err)
    const likelyPathIssue = /can't cd to|__NOT_DIR__|no such file|not a directory|permission denied|failure/i.test(message)
    const metadata = details?.item
      ? ` (${t.permission}: ${details.item.permission || '-'}, ${t.ownerGroup}: ${details.item.ownerGroup || '-'})`
      : ''
    const pathText = details?.targetPath ? ` ${details.targetPath}` : ''

    if (locale === 'zhCN') {
      if (details?.targetPath && likelyPathIssue) {
        return `无法打开远程目录${pathText}${metadata}。可能是目录不存在、不是目录，或者当前账号没有进入权限。原始错误：${message}`
      }
      return `${scope}${pathText}${metadata}失败：${message}`
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

  const handleDeleteProfile = async (
    event: MouseEvent<HTMLButtonElement>,
    profileId: string
  ) => {
    event.stopPropagation()
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

  const runFileAction = async (action: () => Promise<void>) => {
    try {
      setIsBusy(true)
      await action()
      setFileActionDialog(null)
      setFileActionError(null)
    } catch (err) {
      reportError(setFileActionError, '文件操作', err)
    } finally {
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
    setFileActionDialog({ kind: 'new-folder', pane, directoryPath })
  }

  const requestNewFile = (pane: 'local' | 'remote', directoryPath: string) => {
    setFileActionError(null)
    setFileActionDialog({ kind: 'new-file', pane, directoryPath })
  }

  const requestRename = (pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem) => {
    setFileActionError(null)
    setFileActionDialog({
      kind: 'rename',
      target: { pane, path: item.path, name: item.name, type: item.type }
    })
  }

  const requestDelete = (pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>) => {
    setFileActionError(null)
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
        setIsBusy(true)
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
        setIsBusy(false)
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
            mode={editingProfileId ? 'edit' : 'create'}
            form={form}
            setForm={updateForm}
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
    )
  }

  if (isCommandFormWindow) {
    const editingCommand = formWindowMode === 'edit'
      ? workspace.commandTemplates.find((item) => item.id === formWindowCommandId) ?? null
      : null

    return (
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
    )
  }

  if (isConnectionFormWindow) {
    return (
      <ConnectionModal
        errorMessage={formError}
        mode={editingProfileId ? 'edit' : formWindowMode}
        form={form}
        setForm={updateForm}
        standalone
        onSubmit={handleSaveProfile}
        onClose={closeCurrentWindow}
      />
    )
  }

  if (isFileEditorWindow && fileEditor) {
    return (
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
    )
  }

  if (isFileEditorWindow) {
    return (
      <div className="standalone-shell file-editor-window">
        <div className="modal-card file-editor-modal file-editor-modal--dark standalone">
          <div className="modal-header">
            <div className="file-editor-title">
              <span>{fileEditorWindowSource === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
              <strong>{fileEditorWindowName ?? ''}</strong>
            </div>
          </div>
          {fileEditorError ? <div className="modal-error">{fileEditorError}</div> : <div className="file-editor-path">{t.updating}</div>}
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="fs-shell" style={{ '--sidebar-width': `${sidebarWidth}px` } as CSSProperties}>
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

        <aside className="fs-sidebar" style={{ position: 'relative' }}>
          <SystemSidebar activeProfile={activeProfile} activeSession={activeSession} onOpenSystemInfo={handleOpenSystemInfo} />
          <div
            aria-label={t.resizeSidebar}
            className={`sidebar-resizer ${isResizingSidebar ? 'is-active' : ''}`}
            onMouseDown={() => setIsResizingSidebar(true)}
            role="separator"
          />
        </aside>

        <main className={`fs-main ${error ? 'has-status' : 'no-status'}`}>
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
              onClose={() => setShowTransfers(false)}
            />
        ) : null}

        {rootAccessDialog ? (
          <RootAccessModal
            defaultSshUser={rootAccessDialog.sshUser}
            defaultSudoUser={rootAccessDialog.sudoUser}
            errorMessage={rootAccessDialogError}
            onClose={() => {
              setRootAccessDialog(null)
              setRootAccessDialogError(null)
            }}
            onSubmit={handleConfirmRootAccess}
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
          mode={editingProfileId ? 'edit' : 'create'}
          form={form}
          setForm={updateForm}
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
          inputLabel={fileActionDialog.kind === 'delete' ? undefined : t.fileName}
          inputPlaceholder={
            fileActionDialog.kind === 'new-folder' ? t.folderName : t.fileName
          }
          onClose={() => {
            setFileActionDialog(null)
            setFileActionError(null)
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
    </>
  )
}

function appendTerminalTranscript(current: string | undefined, chunk: string) {
  const next = `${current ?? ''}${chunk}`
  if (next.length <= TERMINAL_TRANSCRIPT_LIMIT) {
    return next
  }

  return next.slice(next.length - TERMINAL_TRANSCRIPT_LIMIT)
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
