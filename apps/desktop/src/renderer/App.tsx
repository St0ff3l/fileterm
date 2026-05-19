import { useEffect, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandTemplateInput,
  ConnectionFormMode,
  ConnectionProfile,
  CreateProfileInput,
  FileContentSnapshot,
  LocalFileItem,
  RemoteFileItem,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { defaultForm, emptyState, localPreviewFiles, previewLocalPath, previewState, profileToForm } from './app/app-data'
import { homeTabKey, isActiveTransfer, reorderTabKeys, sessionTabKey, withParentRow } from './app/app-utils'
import { CommandManagerModal } from './features/commands/CommandManagerModal'
import { ConnectionManagerModal } from './features/connections/ConnectionManagerModal'
import { ConnectionModal } from './features/connections/ConnectionModal'
import { FileEditorModal } from './features/files/FileEditorModal'
import { TabBar, type OrderedTabEntry, type TabContextTarget } from './features/layout/TabBar'
import { TabContextMenu } from './features/layout/TabContextMenu'
import { SystemSidebar } from './features/system/SystemSidebar'
import { TransferBar } from './features/transfers/TransferBar'
import { TransferPopover } from './features/transfers/TransferPopover'
import { WorkspaceStage } from './features/workspace/WorkspaceStage'
import { useThemeMode, type ThemeMode } from './hooks/useThemeMode'
import { defaultLocale, setLocale, t, type AppLocale } from './i18n'

type LocalTab =
  | { id: string; kind: 'home'; title: string }
  | { id: string; kind: 'system'; title: string; sessionTabId: string }

export function App() {
  const searchParams = new URLSearchParams(window.location.search)
  const windowMode = searchParams.get('window') ?? 'main'
  const isConnectionManagerWindow = windowMode === 'connection-manager'
  const isCommandManagerWindow = windowMode === 'command-manager'
  const isConnectionFormWindow = windowMode === 'connection-form'
  const formWindowMode = (searchParams.get('mode') as ConnectionFormMode | null) ?? 'create'
  const formWindowProfileId = searchParams.get('profileId')

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
  const [fileEditor, setFileEditor] = useState<FileContentSnapshot | null>(null)
  const [fileEditorError, setFileEditorError] = useState<string | null>(null)
  const [showTransfers, setShowTransfers] = useState(false)
  const [themeMode, setThemeMode] = useState<ThemeMode>('default-dark')
  const [locale, setLocaleState] = useState<AppLocale>(defaultLocale)

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
    setLocalTabs((prev) => prev.map((tab) => {
      if (tab.kind === 'home') {
        return { ...tab, title: t.untitledTab }
      }
      return { ...tab, title: t.systemInfoTabTitle }
    }))
  }, [locale])

  useEffect(() => {
    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
      setActiveLocalTabId(null)
      setTabOrder(['session:preview-tab-ssh'])
      setError(t.browserPreview)
      return
    }

    desktopApi
      .getSnapshot()
      .then(setWorkspace)
      .catch((err: Error) => setError(err.message))
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    const offSnapshot = desktopApi.onWorkspaceSnapshot((snapshot) => {
      applySnapshot(snapshot)
    })

    if (!isConnectionManagerWindow && !isConnectionFormWindow && !isCommandManagerWindow) {
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
  }, [desktopApi, isCommandManagerWindow, isConnectionFormWindow, isConnectionManagerWindow])

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

  const applySnapshot = (snapshot: WorkspaceSnapshot) => {
    setWorkspace(snapshot)
    setError(null)
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
      setFormError((err as Error).message)
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
    } catch (err) {
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
    setLocalTabs((prev) => [
      ...prev,
      {
        id: nextId,
        kind: 'system',
        title: t.systemInfoTabTitle,
        sessionTabId: activeTab.id
      }
    ])
    setTabOrder((prev) => [...prev, homeTabKey(nextId)])
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
      setError((err as Error).message)
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
      setError((err as Error).message)
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
      const content = await desktopApi.readLocalFile(item.path)
      setFileEditor({ path: item.path, name: item.name, source: 'local', content })
      setFileEditorError(null)
    } catch (err) {
      setError(item.type === 'folder' ? t.localLoadFailed : (err as Error).message)
    }
  }

  const openRemoteDirectory = async (tabId: string, targetPath: string) => {
    if (!desktopApi) {
      return
    }

    const snapshot = await desktopApi.openRemotePath(tabId, targetPath)
    applySnapshot(snapshot)
  }

  const openRemoteFileForEdit = async (tabId: string, item: RemoteFileItem) => {
    if (!desktopApi) {
      return
    }
    const content = await desktopApi.readRemoteFile(tabId, item.path)
    setFileEditor({ path: item.path, name: item.name, source: 'remote', content })
    setFileEditorError(null)
  }

  const handleSaveFileEditor = async (content: string) => {
    if (!desktopApi || !fileEditor) {
      return
    }

    try {
      setIsBusy(true)
      if (fileEditor.source === 'local') {
        await desktopApi.writeLocalFile(fileEditor.path, content)
        await openLocalDirectory(localPath)
      } else if (activeTab) {
        const snapshot = await desktopApi.writeRemoteFile(activeTab.id, fileEditor.path, content)
        applySnapshot(snapshot)
      }
      setFileEditor(null)
      setFileEditorError(null)
    } catch (err) {
      setFileEditorError((err as Error).message)
    } finally {
      setIsBusy(false)
    }
  }

  const uploadLocalPaths = async (paths: string[]) => {
    if (!desktopApi || !activeTab || !activeSession) {
      return
    }

    for (const localPath of Array.from(new Set(paths))) {
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
          await openRemoteDirectory(activeTab.id, item.path)
        } else {
          await openRemoteFileForEdit(activeTab.id, item)
        }
      } catch (err) {
        setError((err as Error).message)
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
        setError((err as Error).message)
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
                setError(err.message)
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
          {error ? <div className="status-message">{error}</div> : null}
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
                void openLocalDirectory(targetPath).catch((err: Error) => setError(err.message))
              }}
              onOpenProfile={handleOpenProfile}
              onOpenRemoteItem={handleOpenRemoteItem}
              onOpenRemotePath={handleOpenRemotePath}
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
                setError(err.message)
              })
            }}
            onClose={() => setShowTransfers(false)}
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

      {fileEditor ? (
        <FileEditorModal
          errorMessage={fileEditorError}
          file={fileEditor}
          onClose={() => {
            setFileEditor(null)
            setFileEditorError(null)
          }}
          onSave={handleSaveFileEditor}
        />
      ) : null}
    </>
  )
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
