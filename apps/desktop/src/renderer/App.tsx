import { useEffect, useRef, useState, useTransition, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent } from 'react'
import type {
  ConnectionProfile,
  CreateProfileInput,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  SystemMetrics,
  TransferTask,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { t } from './i18n'
import { TerminalView } from './components/TerminalView'

const emptyState: WorkspaceSnapshot = {
  profiles: [],
  tabs: [],
  activeTabId: null,
  transfers: [],
  sessions: {}
}

const localPreviewFiles: LocalFileItem[] = [
  { path: '/Users/stoffel', name: '..', type: 'folder', modified: '2026/05/15 18:44', size: '-' },
  { path: '/Users/stoffel/Downloads', name: 'Downloads', type: 'folder', modified: '2026/05/15 18:44', size: '-' },
  { path: '/Users/stoffel/Desktop', name: 'Desktop', type: 'folder', modified: '2026/05/15 18:32', size: '-' },
  { path: '/Users/stoffel/release.tar.gz', name: 'release.tar.gz', type: 'file', modified: '2026/05/15 17:18', size: '742 MB' },
  { path: '/Users/stoffel/backup.sql.gz', name: 'backup.sql.gz', type: 'file', modified: '2026/05/15 16:02', size: '1.1 GB' }
]

const previewLocalPath = '/Users/stoffel'

const previewState: WorkspaceSnapshot = {
  profiles: [
    {
      id: 'preview-profile-ssh',
      type: 'ssh',
      name: 'SynologyNAS',
      host: '114.66.28.185',
      port: 22,
      username: 'Stoffel',
      authType: 'privateKey',
      privateKeyPath: '~/.ssh/id_ed25519',
      group: '默认',
      sftpEnabled: true,
      remotePath: '/'
    },
    {
      id: 'preview-profile-ftp',
      type: 'ftp',
      name: 'archive-ftp',
      host: 'ftp.example.net',
      port: 21,
      username: 'deploy',
      secure: false,
      group: 'FTP',
      remotePath: '/incoming'
    }
  ],
  tabs: [
    {
      id: 'preview-tab-ssh',
      sessionType: 'ssh',
      profileId: 'preview-profile-ssh',
      title: '123',
      layout: 'terminal-file',
      status: 'connected'
    }
  ],
  activeTabId: 'preview-tab-ssh',
  transfers: [],
  sessions: {
    'preview-tab-ssh': {
      profileId: 'preview-profile-ssh',
      summary: 'Connected to 192.168.3.197:22',
      terminalTranscript:
        'Linux fnOSNAS-CN 6.18.18-trim #473 SMP PREEMPT_DYNAMIC Thu Apr  9 09:34:02 UTC 2026 x86_64\r\nLast login: Fri May 15 21:57:26 2026 from 127.0.0.1\r\nCould not chdir to home directory /home/Stoffel: No such file or directory\r\nStoffel@fnOSNAS-CN:~$ ',
      remotePath: '/',
      remoteFiles: [
        { path: '/boot', name: 'boot', type: 'folder', modified: '2026-05-11 17:46', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/dev', name: 'dev', type: 'folder', modified: '2026-05-15 07:20', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/etc', name: 'etc', type: 'folder', modified: '2026-05-11 17:46', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/home', name: 'home', type: 'folder', modified: '2024-08-01 16:06', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/run', name: 'run', type: 'folder', modified: '2026-05-15 07:20', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' }
      ],
      connected: true,
      systemMetrics: {
        ip: '192.168.3.197',
        uptime: '4 天',
        load: '0.44, 0.66, 0.62',
        cpuPercent: 10,
        memoryPercent: 68,
        memoryUsage: '7.9G/11.6G',
        swapPercent: 7,
        swapUsage: '290M/4.0G',
        diskRows: [
          { path: '/dev', usage: '5.8G/5.8G' },
          { path: '/run', usage: '1.1G/1.2G' },
          { path: '/', usage: '44G/63G' },
          { path: '/dev/shm', usage: '5.9G/5.9G' },
          { path: '/run/lock', usage: '5.0M/5.0M' }
        ],
        networkInterfaces: ['enp3s0-ovs'],
        activeNetworkInterface: 'enp3s0-ovs',
        networkRates: { tx: '540B', rx: '233B' },
        networkSamples: Array.from({ length: 18 }, (_, index) => ({
          tx: [5, 11, 8, 14, 4, 9, 2, 12, 10, 6, 15, 7, 3, 8, 11, 4, 6, 9][index],
          rx: [10, 19, 13, 22, 6, 24, 8, 15, 18, 12, 20, 9, 5, 13, 16, 8, 11, 14][index]
        })),
        topProcesses: [
          { memory: '3171.4M', cpu: '3.0', command: 'python' },
          { memory: '1309.4M', cpu: '1.0', command: 'next-server' },
          { memory: '536.1M', cpu: '0.6', command: 'python3' },
          { memory: '349.7M', cpu: '0.5', command: 'trim-photos' }
        ]
      }
    }
  }
}

const defaultForm: CreateProfileInput = {
  type: 'ssh',
  name: '',
  host: '',
  port: 22,
  username: '',
  group: '默认',
  remotePath: '/',
  note: '',
  password: '',
  privateKeyPath: '',
  passphrase: '',
  authType: 'password',
  encoding: 'UTF-8',
  backspaceKey: 'ASCII',
  deleteKey: 'VT220',
  enableExecChannel: true,
  secure: false
}

function profileToForm(profile: ConnectionProfile): CreateProfileInput {
  return {
    type: profile.type,
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    group: profile.group,
    remotePath: profile.remotePath,
    note: profile.note ?? '',
    password: profile.password ?? '',
    authType: profile.type === 'ssh' ? profile.authType : 'password',
    privateKeyPath: profile.type === 'ssh' ? profile.privateKeyPath ?? '' : '',
    passphrase: profile.type === 'ssh' ? profile.passphrase ?? '' : '',
    encoding: profile.type === 'ssh' ? profile.encoding ?? 'UTF-8' : 'UTF-8',
    backspaceKey: profile.type === 'ssh' ? profile.backspaceKey ?? 'ASCII' : 'ASCII',
    deleteKey: profile.type === 'ssh' ? profile.deleteKey ?? 'VT220' : 'VT220',
    enableExecChannel: profile.type === 'ssh' ? profile.enableExecChannel ?? true : true,
    secure: profile.type === 'ftp' ? profile.secure : false
  }
}

export function App() {
  const [workspace, setWorkspace] = useState<WorkspaceSnapshot>(emptyState)
  const [error, setError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [, startTransition] = useTransition()
  const [isBusy, setIsBusy] = useState(false)
  const [showForm, setShowForm] = useState(false)
  const [showConnectionManager, setShowConnectionManager] = useState(false)
  const [form, setForm] = useState<CreateProfileInput>(defaultForm)
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null)
  const [localPath, setLocalPath] = useState(previewLocalPath)
  const [localItems, setLocalItems] = useState<LocalFileItem[]>(localPreviewFiles)
  const [homeTabs, setHomeTabs] = useState([{ id: 'home-1' }])
  const [activeHomeTabId, setActiveHomeTabId] = useState<string | null>('home-1')
  const [nextHomeTabNumber, setNextHomeTabNumber] = useState(2)
  const [tabOrder, setTabOrder] = useState<string[]>(['home:home-1'])
  const [draggingTabKey, setDraggingTabKey] = useState<string | null>(null)
  const [sidebarWidth, setSidebarWidth] = useState(214)
  const [isResizingSidebar, setIsResizingSidebar] = useState(false)
  const homeTabsRef = useRef(homeTabs)
  const pendingHomeReplacementKeyRef = useRef<string | null>(null)
  const desktopApi = window.termdock
  const isDesktopRuntime = Boolean(desktopApi?.isDesktop)

  useEffect(() => {
    homeTabsRef.current = homeTabs
  }, [homeTabs])

  useEffect(() => {
    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
      setActiveHomeTabId(null)
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

    desktopApi
      .listLocalDirectory()
      .then(({ path, items }) => {
        setLocalPath(path)
        setLocalItems(withParentRow(path, items))
      })
      .catch(() => setError(t.localLoadFailed))

    return () => {
      offSnapshot()
    }
  }, [desktopApi])

  const activeTab = workspace.tabs.find((tab) => tab.id === workspace.activeTabId) ?? null
  const activeSession = activeHomeTabId ? null : activeTab ? workspace.sessions[activeTab.id] : null
  const activeProfile = !activeHomeTabId && activeTab
    ? workspace.profiles.find((profile) => profile.id === activeTab.profileId) ?? null
    : null

  useEffect(() => {
    const allKeys = [
      ...homeTabs.map((tab) => homeTabKey(tab.id)),
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
  }, [homeTabs, workspace.tabs])

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

  const recentProfiles = workspace.profiles.slice(0, 12)

  const applySnapshot = (snapshot: WorkspaceSnapshot) => {
    startTransition(() => {
      setWorkspace(snapshot)
      setError(null)
      setFormError(null)
    })
  }

  const updateForm = (
    updater: CreateProfileInput | ((prev: CreateProfileInput) => CreateProfileInput)
  ) => {
    setForm((prev) => (typeof updater === 'function' ? updater(prev) : updater))
    setFormError(null)
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

  const handleSaveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (!form.name || !form.host || !form.username || !form.group || !form.remotePath || !Number(form.port)) {
      setFormError(t.fillRequired)
      return
    }

    if (form.type === 'ssh' && form.authType === 'password' && !form.password) {
      setFormError('请填写 SSH 密码。')
      return
    }

    if (form.type === 'ssh' && form.authType === 'privateKey' && !form.privateKeyPath) {
      setFormError('请选择或填写私钥路径。')
      return
    }

    if (!desktopApi) {
      setFormError(t.desktopOnlyCreate)
      return
    }

    try {
      setIsBusy(true)
      const payload = {
        ...form,
        port: Number(form.port)
      }
      const snapshot = editingProfileId
        ? await desktopApi.updateProfile(editingProfileId, payload)
        : await desktopApi.createProfile(payload)
      applySnapshot(snapshot)
      setShowForm(false)
      setEditingProfileId(null)
      setForm(defaultForm)
    } catch (err) {
      setFormError((err as Error).message)
    } finally {
      setIsBusy(false)
    }
  }

  const handleOpenProfile = async (profileId: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = activeHomeTabId
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
        setHomeTabs((prev) => prev.filter((tab) => tab.id !== activeHomeId))
        pendingHomeReplacementKeyRef.current = null
      }
      setActiveHomeTabId(null)
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
      setActiveHomeTabId(null)
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setIsBusy(false)
    }
  }

  const handleCloseTab = async (event: MouseEvent<HTMLButtonElement>, tabId: string) => {
    event.stopPropagation()
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.closeTab(tabId)
      applySnapshot(snapshot)
      if (snapshot.activeTabId === null) {
        setHomeTabs((prev) => prev.length ? prev : [{ id: 'home-1' }])
        setTabOrder((prev) => {
          const filtered = prev.filter((key) => key !== sessionTabKey(tabId))
          return filtered.some((key) => key.startsWith('home:')) ? filtered : ['home:home-1', ...filtered]
        })
        setActiveHomeTabId((prev) => prev ?? homeTabsRef.current.at(-1)?.id ?? 'home-1')
      }
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setIsBusy(false)
    }
  }

  const handleActivateHome = (homeTabId: string) => {
    startTransition(() => {
      setError(null)
      setActiveHomeTabId(homeTabId)
    })
  }

  const handleAddHomeTab = () => {
    const nextId = `home-${nextHomeTabNumber}`
    const nextKey = homeTabKey(nextId)

    setHomeTabs((prev) => [...prev, { id: nextId }])
    setTabOrder((prev) => [...prev, nextKey])
    setNextHomeTabNumber((prev) => prev + 1)
    setActiveHomeTabId(nextId)
    setError(null)
  }

  const handleCloseHomeTab = (event: MouseEvent<HTMLButtonElement>, homeTabId: string) => {
    event.stopPropagation()

    setHomeTabs((prev) => {
      const remaining = prev.filter((tab) => tab.id !== homeTabId)

      if (remaining.length === 0 && workspace.tabs.length === 0) {
        setActiveHomeTabId('home-1')
        setNextHomeTabNumber(2)
        setTabOrder((prev) => {
          const filtered = prev.filter((key) => key !== homeTabKey(homeTabId))
          return filtered.includes('home:home-1') ? filtered : ['home:home-1', ...filtered]
        })
        return [{ id: 'home-1' }]
      }

      if (activeHomeTabId === homeTabId) {
        setActiveHomeTabId(remaining.at(-1)?.id ?? null)
      }

      setTabOrder((prev) => prev.filter((key) => key !== homeTabKey(homeTabId)))
      return remaining
    })
  }

  const handleDragStart = (tabKey: string) => {
    setDraggingTabKey(tabKey)
  }

  const handleDragEnter = (targetKey: string) => {
    setTabOrder((prev) => reorderTabKeys(prev, draggingTabKey, targetKey))
  }

  const handleDragEnd = () => {
    setDraggingTabKey(null)
  }

  const orderedTabs = tabOrder
    .map((key) => {
      if (key.startsWith('home:')) {
        const id = key.slice(5)
        const homeTab = homeTabs.find((tab) => tab.id === id)
        return homeTab ? { key, kind: 'home' as const, id: homeTab.id } : null
      }

      const id = key.slice(8)
      const sessionTab = workspace.tabs.find((tab) => tab.id === id)
      return sessionTab ? { key, kind: 'session' as const, tab: sessionTab } : null
    })
    .filter((item): item is NonNullable<typeof item> => item !== null)

  const handleDropUpload = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    const fileNames = Array.from(event.dataTransfer.files).map((file) => file.name).filter(Boolean)
    if (!fileNames.length) {
      setError(t.desktopOnlyUpload)
      return
    }

    if (!desktopApi) {
      setError(t.desktopOnlyUpload)
      return
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.queueUpload(fileNames)
      applySnapshot(snapshot)
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setIsBusy(false)
    }
  }

  const handleOpenLocalItem = async (item: LocalFileItem) => {
    if (item.type !== 'folder') {
      return
    }

    if (!desktopApi) {
      setLocalPath(item.path)
      return
    }

    try {
      const { path, items } = await desktopApi.listLocalDirectory(item.path)
      setLocalPath(path)
      setLocalItems(withParentRow(path, items))
    } catch {
      setError(t.localLoadFailed)
    }
  }

  return (
    <>
      <div className="fs-shell" style={{ '--sidebar-width': `${sidebarWidth}px` } as CSSProperties}>
        <header className="fs-tabbar">
          <div className="titlebar-brand">
            <strong>TermDock</strong>
          </div>
          <div className="titlebar-tabarea">
            <button
              aria-label="Open connection manager"
              className="tabbar-folder-button"
              onClick={() => setShowConnectionManager(true)}
              title="连接管理器"
              type="button"
            >
              <AppIcon name="connections" size={16} />
            </button>
            <div className="fs-tabs">
              {orderedTabs.map((entry, index) => (
                entry.kind === 'home' ? (
                  <div
                    key={entry.key}
                    className={`fs-tab home-tab ${activeHomeTabId === entry.id ? 'active' : ''}`}
                    draggable
                    onClick={() => handleActivateHome(entry.id)}
                    onDragStart={() => handleDragStart(entry.key)}
                    onDragEnter={() => handleDragEnter(entry.key)}
                    onDragEnd={handleDragEnd}
                    onDragOver={(event) => event.preventDefault()}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        handleActivateHome(entry.id)
                      }
                    }}
                    role="button"
                    tabIndex={0}
                  >
                    <span>{index + 1}</span>
                    <strong>{t.newTab}</strong>
                    <button
                      aria-label={`Close ${t.newTab}`}
                      className="tab-close"
                      onClick={(event) => handleCloseHomeTab(event, entry.id)}
                      type="button"
                    >
                      ×
                    </button>
                  </div>
                ) : (
                  <div
                    key={entry.key}
                    className={`fs-tab session-tab ${entry.tab.id === workspace.activeTabId && !activeHomeTabId ? 'active' : ''}`}
                    draggable
                    onClick={() => handleActivateTab(entry.tab.id)}
                    onDragStart={() => handleDragStart(entry.key)}
                    onDragEnter={() => handleDragEnter(entry.key)}
                    onDragEnd={handleDragEnd}
                    onDragOver={(event) => event.preventDefault()}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        handleActivateTab(entry.tab.id)
                      }
                    }}
                    role="button"
                    tabIndex={0}
                  >
                    <span>{index + 1}</span>
                    <strong>{entry.tab.title}</strong>
                    <span className={`tab-dot ${tabStatusClass(entry.tab.status)}`} />
                    <button
                      aria-label={`Close ${entry.tab.title}`}
                      className="tab-close"
                      onClick={(event) => handleCloseTab(event, entry.tab.id)}
                      type="button"
                    >
                      ×
                    </button>
                  </div>
                )
              ))}
              <button className="add-tab" type="button" onClick={handleAddHomeTab}>+</button>
            </div>
            <div className="window-tools">
              <button title="Grid" type="button"><AppIcon name="grid" /></button>
            </div>
          </div>
        </header>

        <aside className="fs-sidebar" style={{ position: 'relative' }}>
          <SystemPanel activeProfile={activeProfile} activeSession={activeSession} />
          <DiskPanel activeSession={activeSession} />
          <div
            aria-label="Resize sidebar"
            className={`sidebar-resizer ${isResizingSidebar ? 'is-active' : ''}`}
            onMouseDown={() => setIsResizingSidebar(true)}
            role="separator"
          />
        </aside>

        <main className={`fs-main ${error ? 'has-status' : 'no-status'}`}>
          {error ? <div className="status-message">{error}</div> : null}

          <div className="workspace-stage">
            {activeTab && activeSession ? (
              <SessionWorkspace
                activeTab={activeTab}
                activeSession={activeSession}
                localItems={localItems}
                localPath={localPath}
                onOpenLocalItem={handleOpenLocalItem}
                onOpenRemoteItem={async (item) => {
                  if (item.type !== 'folder' || !desktopApi) {
                    return
                  }

                  try {
                    setIsBusy(true)
                    const snapshot = await desktopApi.openRemotePath(activeTab.id, item.path)
                    applySnapshot(snapshot)
                  } catch (err) {
                    setError((err as Error).message)
                  } finally {
                    setIsBusy(false)
                  }
                }}
                onDropUpload={handleDropUpload}
              />
            ) : (
              <HomeWorkspace
                profiles={recentProfiles}
                isDesktopRuntime={isDesktopRuntime}
                onCreate={openCreateModal}
                onOpen={handleOpenProfile}
              />
            )}
          </div>
        </main>
        <TransferBar transfers={workspace.transfers} isPending={isBusy} />
      </div>

      {showConnectionManager ? (
        <ConnectionManagerModal
          profiles={workspace.profiles}
          onClose={() => setShowConnectionManager(false)}
          onCreate={() => {
            setShowConnectionManager(false)
            openCreateModal()
          }}
          onDelete={handleDeleteProfile}
          onEdit={(profile) => {
            setShowConnectionManager(false)
            openEditModal(profile)
          }}
          onOpen={(profileId) => {
            setShowConnectionManager(false)
            void handleOpenProfile(profileId)
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
    </>
  )
}

function SystemPanel({
  activeProfile,
  activeSession
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
}) {
  const metrics = activeSession?.systemMetrics
  const internalIp = metrics?.ip || '-'
  const accessAddress = activeProfile?.host || activeSession?.accessHost || '-'
  const isConnected = Boolean(activeSession?.connected)

  return (
    <section className="sys-card">
      <div className="connection-summary">
        <div className="sync-row connection-status">
          <span>{t.syncStatus}</span>
          <i className={`sync-dot ${isConnected ? 'online' : 'offline'}`} />
        </div>
        <AddressLine label={t.privateIp} value={internalIp} />
        <AddressLine label={t.accessAddress} value={accessAddress} />
      </div>
      <button className="system-title" type="button">{t.systemInfo}</button>
      <div className="metric-line"><span>{t.running}</span><strong>{metrics?.uptime ?? '-'}</strong></div>
      <div className="metric-line"><span>{t.load}</span><strong>{metrics?.load ?? '-'}</strong></div>
      <Meter label={t.cpu} value={metrics?.cpuPercent ?? 0} tone="green" caption={metrics ? `${metrics.cpuPercent}%` : '0%'} />
      <Meter label={t.memory} value={metrics?.memoryPercent ?? 0} tone="orange" caption={metrics?.memoryUsage ?? '0/0'} />
      <Meter label={t.swap} value={metrics?.swapPercent ?? 0} tone="yellow" caption={metrics?.swapUsage ?? '0/0'} />
      <div className="mini-tabs">
        <span>{t.memory}</span>
        <span>{t.cpu}</span>
        <span>{t.command}</span>
      </div>
      <ProcessTable rows={metrics?.topProcesses ?? []} />
      <NetworkPanel metrics={metrics} />
    </section>
  )
}

function AddressLine({ label, value }: { label: string; value: string }) {
  const canCopy = value && value !== '-'

  return (
    <div className="address-row">
      <span>{label}</span>
      <strong title={value}>{value}</strong>
      <button
        className="copy-link"
        disabled={!canCopy}
        onClick={() => {
          if (canCopy) {
            copyText(value)
          }
        }}
        type="button"
      >
        复制
      </button>
    </div>
  )
}

function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    void navigator.clipboard.writeText(value)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.setAttribute('readonly', '')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
}

function Meter({ label, value, tone, caption }: { label: string; value: number; tone: string; caption: string }) {
  return (
    <div className="meter-row">
      <span>{label}</span>
      <div className="meter-track"><i className={`meter-fill ${tone}`} style={{ width: `${value}%` }} /></div>
      <strong>{caption}</strong>
    </div>
  )
}

function DiskPanel({ activeSession }: { activeSession: SessionSnapshot | null }) {
  const rows = activeSession?.systemMetrics?.diskRows ?? []

  return (
    <section className="disk-table">
      <div className="disk-head"><span>{t.path}</span><span>{t.availableSize}</span></div>
      {rows.map((row) => (
        <div className="disk-row" key={row.path}><span>{row.path}</span><span>{row.usage}</span></div>
      ))}
    </section>
  )
}

function ProcessTable({ rows }: { rows: SystemMetrics['topProcesses'] }) {
  return (
    <div className="process-table">
      {rows.length ? rows.map((row) => (
        <div className="process-row" key={`${row.command}-${row.memory}`}>
          <span>{row.memory}</span>
          <span>{row.cpu}</span>
          <span>{row.command}</span>
        </div>
      )) : <div className="process-empty" />}
    </div>
  )
}

function NetworkPanel({ metrics }: { metrics?: SystemMetrics }) {
  const [selectedInterface, setSelectedInterface] = useState(metrics?.activeNetworkInterface ?? '')

  useEffect(() => {
    setSelectedInterface(metrics?.activeNetworkInterface ?? '')
  }, [metrics?.activeNetworkInterface])

  return (
    <>
      <div className="network-panel">
        <div>
          <span className="up">↑{metrics?.networkRates.tx ?? '0B'}</span>
          <span className="down">↓{metrics?.networkRates.rx ?? '0B'}</span>
        </div>
        <select
          className="network-select"
          value={selectedInterface}
          onChange={(event) => setSelectedInterface(event.target.value)}
        >
          {(metrics?.networkInterfaces.length ? metrics.networkInterfaces : ['-']).map((name) => (
            <option key={name} value={name}>{name}</option>
          ))}
        </select>
      </div>
      <div className="grid-chart">
        {(metrics?.networkSamples.length ? metrics.networkSamples : Array.from({ length: 18 }, () => ({ rx: 0, tx: 0 }))).map((sample, index) => {
          const maxValue = Math.max(...(metrics?.networkSamples ?? [{ rx: 1, tx: 1 }]).flatMap((item) => [item.rx, item.tx]), 1)
          return (
            <div className="grid-chart-bar" key={`${selectedInterface}-${index}`}>
              <i className="tx-bar" style={{ height: `${Math.max(4, (sample.tx / maxValue) * 100)}%` }} />
              <i className="rx-bar" style={{ height: `${Math.max(4, (sample.rx / maxValue) * 100)}%` }} />
            </div>
          )
        })}
      </div>
    </>
  )
}

function HomeWorkspace({
  profiles,
  isDesktopRuntime,
  onCreate,
  onOpen
}: {
  profiles: ConnectionProfile[]
  isDesktopRuntime: boolean
  onCreate(): void
  onOpen(profileId: string): void
}) {
  return (
    <section className="home-workspace">
      <div className="quick-panel">
        <div className="quick-header">
          <strong>{t.quickConnect}</strong>
          <div>
            <button className="flat-button" type="button" disabled={!isDesktopRuntime} onClick={onCreate}>{t.newConnection}</button>
          </div>
        </div>
        <div className="quick-list">
          {profiles.map((profile) => (
            <div
              className="quick-row"
              key={profile.id}
              onClick={() => onOpen(profile.id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  onOpen(profile.id)
                }
              }}
              role="button"
              tabIndex={0}
            >
              <span className="host-icon"><AppIcon name="server" /></span>
              <strong>{profile.name}</strong>
              <span>{profile.note || '/'}</span>
              <span>{profile.username}</span>
              <small>{profile.type.toUpperCase()}</small>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}

function ConnectionManagerModal({
  profiles,
  onClose,
  onCreate,
  onDelete,
  onEdit,
  onOpen
}: {
  profiles: ConnectionProfile[]
  onClose(): void
  onCreate(): void
  onDelete(event: MouseEvent<HTMLButtonElement>, profileId: string): void
  onEdit(profile: ConnectionProfile): void
  onOpen(profileId: string): void
}) {
  return (
    <div className="modal-backdrop">
      <div className="modal-card manager-modal">
        <div className="modal-header">
          <span>连接管理器</span>
          <button className="icon-button" onClick={onClose} type="button">×</button>
        </div>
        <div className="manager-toolbar">
          <button className="primary-button" type="button" onClick={onCreate}>新建连接</button>
        </div>
        <div className="manager-table">
          <div className="manager-head">
            <span>名称</span>
            <span>主机</span>
            <span>端口</span>
            <span>用户</span>
            <span>类型</span>
            <span>备注</span>
            <span>操作</span>
          </div>
          {profiles.map((profile) => (
            <div
              className="manager-row"
              key={profile.id}
              onDoubleClick={() => onOpen(profile.id)}
              onClick={() => onOpen(profile.id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  onOpen(profile.id)
                }
              }}
              role="button"
              tabIndex={0}
            >
              <span>{profile.name}</span>
              <span>{profile.host}</span>
              <span>{profile.port}</span>
              <span>{profile.username}</span>
              <span>{profile.type.toUpperCase()}</span>
              <span>{profile.note || '/'}</span>
              <span className="manager-actions">
                <button
                  className="flat-button compact"
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation()
                    onEdit(profile)
                  }}
                >
                  编辑
                </button>
                <button
                  className="flat-button compact danger"
                  type="button"
                  onClick={(event) => onDelete(event, profile.id)}
                >
                  删除
                </button>
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function SessionWorkspace({
  activeTab,
  activeSession,
  localItems,
  localPath,
  onOpenLocalItem,
  onOpenRemoteItem,
  onDropUpload
}: {
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  localItems: LocalFileItem[]
  localPath: string
  onOpenLocalItem(item: LocalFileItem): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
}) {
  const isFileOnly = activeTab.layout === 'file-only'
  const [filePanelHeight, setFilePanelHeight] = useState(218)
  const workspaceRef = useRef<HTMLElement | null>(null)
  const isResizingFilePanel = useRef(false)
  const hasAlignedFilePanel = useRef(false)

  useEffect(() => {
    if (isFileOnly) {
      return
    }

    const onMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFilePanel.current || !workspaceRef.current) {
        return
      }

      const rect = workspaceRef.current.getBoundingClientRect()
      const nextHeight = rect.bottom - event.clientY
      const maxHeight = Math.max(140, rect.height - 160)
      setFilePanelHeight(Math.min(maxHeight, Math.max(140, nextHeight)))
    }

    const onMouseUp = () => {
      isResizingFilePanel.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isFileOnly])

  useEffect(() => {
    hasAlignedFilePanel.current = false
  }, [activeTab.id])

  useEffect(() => {
    if (isFileOnly || hasAlignedFilePanel.current) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      const workspaceRect = workspaceRef.current?.getBoundingClientRect()
      const diskHeadRect = document.querySelector('.disk-head')?.getBoundingClientRect()

      if (!workspaceRect || !diskHeadRect) {
        return
      }

      const nextHeight = workspaceRect.bottom - diskHeadRect.top
      const maxHeight = Math.max(140, workspaceRect.height - 160)

      if (nextHeight >= 140 && nextHeight <= maxHeight) {
        setFilePanelHeight(nextHeight)
        hasAlignedFilePanel.current = true
      }
    })

    return () => window.cancelAnimationFrame(frame)
  }, [isFileOnly, activeTab.id])

  return (
    <section
      className={`session-workspace ${isFileOnly ? 'file-only' : ''}`}
      ref={workspaceRef}
      style={{ '--file-panel-height': `${filePanelHeight}px` } as CSSProperties}
    >
      {!isFileOnly ? (
        <div className="terminal-area">
          <TerminalView
            tabId={activeTab.id}
            initialText={activeSession.terminalTranscript ?? ''}
          />
        </div>
      ) : null}
      {!isFileOnly ? (
        <div
          className="session-split-resizer"
          onMouseDown={() => {
            isResizingFilePanel.current = true
            document.body.style.cursor = 'row-resize'
            document.body.style.userSelect = 'none'
          }}
          role="separator"
        />
      ) : null}
      <FileManager
        activeSession={activeSession}
        localItems={localItems}
        localPath={localPath}
        onOpenLocalItem={onOpenLocalItem}
        onOpenRemoteItem={onOpenRemoteItem}
        onDropUpload={onDropUpload}
      />
    </section>
  )
}

function FileManager({
  activeSession,
  localItems,
  localPath,
  onOpenLocalItem,
  onOpenRemoteItem,
  onDropUpload
}: {
  activeSession: SessionSnapshot
  localItems: LocalFileItem[]
  localPath: string
  onOpenLocalItem(item: LocalFileItem): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
}) {
  const [localPaneWidth, setLocalPaneWidth] = useState(230)
  const splitRef = useRef<HTMLDivElement | null>(null)
  const isResizingFileSplit = useRef(false)

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFileSplit.current || !splitRef.current) return

      const rect = splitRef.current.getBoundingClientRect()
      const minLocalWidth = 180
      const minRemoteWidth = 320
      const maxLocalWidth = Math.max(minLocalWidth, rect.width - minRemoteWidth)
      const nextWidth = Math.min(maxLocalWidth, Math.max(minLocalWidth, event.clientX - rect.left))
      setLocalPaneWidth(nextWidth)
    }

    const handleMouseUp = () => {
      if (!isResizingFileSplit.current) return
      isResizingFileSplit.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [])

  return (
    <div className="file-manager" onDragOver={(event) => event.preventDefault()} onDrop={onDropUpload}>
      <div className="file-tabs">
        <button className="active" type="button">{t.file}</button>
        <button type="button">{t.command}</button>
        <span className="file-current-path">{activeSession.remotePath}</span>
        <div className="file-tab-actions">
          <button title={t.history} type="button"><AppIcon name="history" /></button>
          <button title="刷新" type="button"><AppIcon name="refresh" /></button>
          <button title="上传到上级目录" type="button"><AppIcon name="arrowUp" /></button>
          <button title="下载到本地" type="button"><AppIcon name="arrowDown" /></button>
          <button title={t.upload} type="button"><AppIcon name="upload" /></button>
        </div>
      </div>
      <div
        className="file-split"
        ref={splitRef}
        style={{ '--local-pane-width': `${localPaneWidth}px` } as CSSProperties}
      >
        <div className="local-pane">
          <div className="pane-title">{t.localComputer}<span>{localPath}</span></div>
          <LocalFileTable rows={localItems} onOpenItem={onOpenLocalItem} />
        </div>
        <div
          className="file-split-resizer"
          onMouseDown={() => {
            isResizingFileSplit.current = true
            document.body.style.cursor = 'col-resize'
            document.body.style.userSelect = 'none'
          }}
          role="separator"
        />
        <div className="remote-pane">
          <div className="pane-title">{t.remoteHost}<span>{t.dragUpload}</span></div>
          <FileTable rows={activeSession.remoteFiles} onOpenItem={onOpenRemoteItem} />
        </div>
      </div>
    </div>
  )
}

function FileTable({
  rows,
  compact = false,
  onOpenItem
}: {
  rows: RemoteFileItem[]
  compact?: boolean
  onOpenItem?(item: RemoteFileItem): void
}) {
  return (
    <table className={`fs-file-table ${compact ? 'compact' : ''}`}>
      <thead>
        <tr>
          <th>{t.fileName}</th>
          {!compact ? <th>{t.size}</th> : null}
          {!compact ? <th>{t.type}</th> : null}
          {!compact ? <th>{t.modifiedAt}</th> : null}
          {!compact ? <th>{t.permission}</th> : null}
          {!compact ? <th>{t.ownerGroup}</th> : null}
        </tr>
      </thead>
      <tbody>
        {rows.length ? rows.map((row) => (
          <tr
            key={row.path}
            className={row.type === 'folder' ? 'is-folder' : ''}
            onDoubleClick={() => onOpenItem?.(row)}
          >
            <td><span className="file-icon"><AppIcon name={row.type === 'folder' ? 'folder' : 'file'} /></span>{row.name}</td>
            {!compact ? <td>{row.size}</td> : null}
            {!compact ? <td>{row.type === 'folder' ? t.folder : row.type}</td> : null}
            {!compact ? <td>{row.modified}</td> : null}
            {!compact ? <td>{row.permission ?? ''}</td> : null}
            {!compact ? <td>{row.ownerGroup ?? ''}</td> : null}
          </tr>
        )) : (
          <tr><td colSpan={compact ? 1 : 6}>{t.emptyFiles}</td></tr>
        )}
      </tbody>
    </table>
  )
}

function LocalFileTable({
  rows,
  onOpenItem
}: {
  rows: LocalFileItem[]
  onOpenItem(item: LocalFileItem): void
}) {
  return (
    <table className="fs-file-table compact">
      <thead>
        <tr>
          <th>{t.fileName}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr
            key={`${row.path}:${row.name}`}
            onDoubleClick={() => onOpenItem(row)}
          >
            <td><span className="file-icon"><AppIcon name={row.type === 'folder' ? 'folder' : 'file'} /></span>{row.name}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function TransferBar({ transfers, isPending }: { transfers: TransferTask[]; isPending: boolean }) {
  return (
    <footer className="transfer-strip">
      <strong>{t.transferTasks}</strong>
      <span>{isPending ? '更新中...' : `${runningTransfers(transfers)} ${t.runningTasks}`}</span>
    </footer>
  )
}

function AppIcon({
  name,
  size = 14
}: {
  name: 'grid' | 'menu' | 'server' | 'connections' | 'folder' | 'file' | 'history' | 'refresh' | 'upload' | 'arrowUp' | 'arrowDown'
  size?: number
}) {
  const commonProps = {
    fill: 'none',
    stroke: 'currentColor',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    strokeWidth: 1.8
  }

  return (
    <svg aria-hidden="true" className={`app-icon app-icon-${name}`} height={size} viewBox="0 0 16 16" width={size}>
      {name === 'grid' ? (
        <>
          <rect {...commonProps} x="2.25" y="2.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="9.25" y="2.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="2.25" y="9.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="9.25" y="9.25" width="4.5" height="4.5" />
        </>
      ) : null}
      {name === 'menu' ? (
        <>
          <path {...commonProps} d="M3 4.5h10" />
          <path {...commonProps} d="M3 8h10" />
          <path {...commonProps} d="M3 11.5h10" />
        </>
      ) : null}
      {name === 'server' ? (
        <>
          <rect {...commonProps} x="2.5" y="2.5" width="11" height="4" rx="1.2" />
          <rect {...commonProps} x="2.5" y="9.5" width="11" height="4" rx="1.2" />
          <path {...commonProps} d="M4.5 4.5h.01M4.5 11.5h.01" />
          <path {...commonProps} d="M8 6.5v3" />
        </>
      ) : null}
      {name === 'connections' ? (
        <>
          <rect {...commonProps} x="2.8" y="3" width="4.2" height="4.2" rx="1" />
          <rect {...commonProps} x="9" y="3" width="4.2" height="4.2" rx="1" />
          <rect {...commonProps} x="5.9" y="9" width="4.2" height="4.2" rx="1" />
          <path {...commonProps} d="M7 5.1h2" />
          <path {...commonProps} d="M5.2 7.2 6.8 9" />
          <path {...commonProps} d="M10.8 7.2 9.2 9" />
        </>
      ) : null}
      {name === 'folder' ? (
        <path {...commonProps} d="M2.5 4.5h3l1.4 1.6h6.6v5.8a1.1 1.1 0 0 1-1.1 1.1H3.6a1.1 1.1 0 0 1-1.1-1.1V5.6a1.1 1.1 0 0 1 1.1-1.1Z" />
      ) : null}
      {name === 'file' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
        </>
      ) : null}
      {name === 'history' ? (
        <>
          <path {...commonProps} d="M3.2 8a4.8 4.8 0 1 0 1.4-3.4" />
          <path {...commonProps} d="M3 3.5v2.8h2.8" />
        </>
      ) : null}
      {name === 'refresh' ? (
        <>
          <path {...commonProps} d="M12.8 6A4.9 4.9 0 0 0 4.4 4.2" />
          <path {...commonProps} d="M4.2 2.9v2.8H7" />
          <path {...commonProps} d="M3.2 10A4.9 4.9 0 0 0 11.6 11.8" />
          <path {...commonProps} d="M11.8 13.1v-2.8H9" />
        </>
      ) : null}
      {name === 'arrowUp' ? (
        <>
          <path {...commonProps} d="M8 12.5V3.5" />
          <path {...commonProps} d="M4.5 7 8 3.5 11.5 7" />
        </>
      ) : null}
      {name === 'arrowDown' ? (
        <>
          <path {...commonProps} d="M8 3.5v9" />
          <path {...commonProps} d="M4.5 9 8 12.5 11.5 9" />
        </>
      ) : null}
      {name === 'upload' ? (
        <>
          <path {...commonProps} d="M8 10.8V3.5" />
          <path {...commonProps} d="M4.7 6.8 8 3.5l3.3 3.3" />
          <path {...commonProps} d="M3 12.5h10" />
        </>
      ) : null}
    </svg>
  )
}

function ConnectionModal({
  errorMessage,
  mode,
  form,
  setForm,
  onSubmit,
  onClose
}: {
  errorMessage: string | null
  mode: 'create' | 'edit'
  form: CreateProfileInput
  setForm(value: CreateProfileInput | ((prev: CreateProfileInput) => CreateProfileInput)): void
  onSubmit(event: FormEvent<HTMLFormElement>): void
  onClose(): void
}) {
  const [section, setSection] = useState<'ssh' | 'terminal' | 'proxy' | 'tunnel'>('ssh')

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-modal">
        <div className="modal-header">
          <span>{mode === 'edit' ? '编辑连接' : t.newConnection}</span>
          <button className="icon-button" onClick={onClose} type="button">×</button>
        </div>
        <div className="ssh-modal-body">
          <aside className="ssh-modal-nav">
            <button className={section === 'ssh' ? 'active' : ''} type="button" onClick={() => setSection('ssh')}>SSH连接</button>
            <button className={section === 'terminal' ? 'active' : ''} type="button" onClick={() => setSection('terminal')}>终端</button>
            <button className={section === 'proxy' ? 'active' : ''} type="button" onClick={() => setSection('proxy')}>代理服务器</button>
            <button className={section === 'tunnel' ? 'active' : ''} type="button" onClick={() => setSection('tunnel')}>隧道</button>
          </aside>
          <form className="ssh-form-shell" onSubmit={onSubmit}>
            {section === 'ssh' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>常规</legend>
                  <div className="ssh-grid ssh-grid-general">
                    <label className="span-2">名称:<input value={form.name} onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))} /></label>
                    <label className="span-2">主机:<input value={form.host} onChange={(event) => setForm((prev) => ({ ...prev, host: event.target.value }))} /></label>
                    <label className="narrow">端口:<input inputMode="numeric" value={form.port || ''} onChange={(event) => setForm((prev) => ({ ...prev, port: Number(event.target.value.replace(/\D/g, '')) }))} /></label>
                    <label className="full">备注:<textarea value={form.note ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, note: event.target.value }))} /></label>
                  </div>
                </fieldset>
                <fieldset className="ssh-fieldset">
                  <legend>认证</legend>
                  <div className="ssh-grid ssh-grid-auth">
                    <label>方法:
                      <select value={form.authType} onChange={(event) => setForm((prev) => ({ ...prev, authType: event.target.value as 'password' | 'privateKey' }))}>
                        <option value="password">密码</option>
                        <option value="privateKey">私钥</option>
                      </select>
                    </label>
                    <label>用户名:<input value={form.username} onChange={(event) => setForm((prev) => ({ ...prev, username: event.target.value }))} /></label>
                    <label className="span-2">密码:<input type="password" value={form.password ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, password: event.target.value }))} /></label>
                    <label className="full">私钥:<input value={form.privateKeyPath ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, privateKeyPath: event.target.value }))} /></label>
                  </div>
                </fieldset>
                <fieldset className="ssh-fieldset">
                  <legend>高级</legend>
                  <label className="ssh-checkbox">
                    <input checked={Boolean(form.enableExecChannel)} type="checkbox" onChange={(event) => setForm((prev) => ({ ...prev, enableExecChannel: event.target.checked }))} />
                    <span>启用Exec Channel(若连接上就被断开,请关闭该项,比如跳板机)</span>
                  </label>
                </fieldset>
              </div>
            ) : null}
            {section === 'terminal' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset narrow">
                  <legend>终端</legend>
                  <div className="ssh-grid single">
                    <label>字符编码:
                      <select value={form.encoding} onChange={(event) => setForm((prev) => ({ ...prev, encoding: event.target.value }))}>
                        <option value="UTF-8">UTF-8</option>
                        <option value="GBK">GBK</option>
                      </select>
                    </label>
                    <div className="terminal-key-box">
                      <strong>按键序列(解决退格/删除键失效,乱码问题):</strong>
                      <label>Backspace退格键
                        <select value={form.backspaceKey} onChange={(event) => setForm((prev) => ({ ...prev, backspaceKey: event.target.value }))}>
                          <option value="ASCII">ASCII - Backspace</option>
                          <option value="DEL">DEL - Backspace</option>
                        </select>
                      </label>
                      <label>Delete删除键
                        <select value={form.deleteKey} onChange={(event) => setForm((prev) => ({ ...prev, deleteKey: event.target.value }))}>
                          <option value="VT220">VT220 - Delete</option>
                          <option value="ASCII">ASCII - Delete</option>
                        </select>
                      </label>
                    </div>
                  </div>
                </fieldset>
              </div>
            ) : null}
            {section === 'proxy' ? <div className="ssh-placeholder">代理服务器功能稍后接入</div> : null}
            {section === 'tunnel' ? <div className="ssh-placeholder">隧道功能稍后接入</div> : null}
            {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
            <div className="form-actions ssh-actions">
              <button className="flat-button" onClick={onClose} type="button">{t.cancel}</button>
              <button className="primary-button" type="submit">{mode === 'edit' ? '保存修改' : t.saveConnection}</button>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}

function runningTransfers(transfers: TransferTask[]) {
  return transfers.filter((transfer) => transfer.status === 'running').length
}

function homeTabKey(id: string) {
  return `home:${id}`
}

function sessionTabKey(id: string) {
  return `session:${id}`
}

function reorderTabKeys(keys: string[], draggingKey: string | null, targetKey: string) {
  if (!draggingKey || draggingKey === targetKey) {
    return keys
  }

  const draggingIndex = keys.indexOf(draggingKey)
  const targetIndex = keys.indexOf(targetKey)
  if (draggingIndex === -1 || targetIndex === -1) {
    return keys
  }

  const next = [...keys]
  next.splice(draggingIndex, 1)
  next.splice(targetIndex, 0, draggingKey)
  return next
}

function tabStatusClass(status: WorkspaceTab['status']) {
  if (status === 'connected') {
    return 'connected'
  }
  if (status === 'error' || status === 'closed') {
    return 'disconnected'
  }
  if (status === 'connecting') {
    return 'connecting'
  }
  return 'idle'
}

function withParentRow(dirPath: string, items: LocalFileItem[]) {
  const parentPath = dirPath.includes('/') ? dirPath.split('/').slice(0, -1).join('/') || '/' : dirPath
  return dirPath === '/' ? items : [
    {
      path: parentPath,
      name: '..',
      type: 'folder' as const,
      modified: '',
      size: '-'
    },
    ...items
  ]
}
