import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  ConnectionFolder,
  ConnectionFormMode,
  ConnectionProfile,
  CreateProfileInput,
  FileTermDesktopApi,
  SshConnectionDefaults
} from '@fileterm/core'
import { defaultForm, profileToForm } from '../app/app-data'
import { t } from '../i18n'

export type WindowCloseConfirmState = {
  isQuit: boolean
  hasActiveConnections: boolean
}

type UseWorkspaceModalsOptions = {
  desktopApi?: FileTermDesktopApi
  folders: ConnectionFolder[]
  formWindowMode: ConnectionFormMode
  formWindowProfileId: string | null
  hasLoadedInitialSnapshot: boolean
  isConnectionFormWindow: boolean
  connectionDefaults: SshConnectionDefaults
  profiles: ConnectionProfile[]
}

function collectConnectionGroups(folderNames: string[], profileGroups: string[], currentGroup?: string) {
  const groups = new Set<string>([t.defaultGroup])

  for (const name of folderNames) {
    const value = name.trim()
    if (value) {
      groups.add(value)
    }
  }

  for (const group of profileGroups) {
    const value = group.trim()
    if (value) {
      groups.add(value)
    }
  }

  if (currentGroup?.trim()) {
    groups.add(currentGroup.trim())
  }

  return [...groups]
}

function createDefaultForm(connectionDefaults: SshConnectionDefaults): CreateProfileInput {
  return { ...defaultForm, ...connectionDefaults, group: t.defaultGroup }
}

export function useWorkspaceModals({
  desktopApi,
  folders,
  formWindowMode,
  formWindowProfileId,
  hasLoadedInitialSnapshot,
  isConnectionFormWindow,
  connectionDefaults,
  profiles
}: UseWorkspaceModalsOptions) {
  const [showConnectionForm, setShowConnectionForm] = useState(false)
  const [showConnectionManager, setShowConnectionManager] = useState(false)
  const [showCommandManager, setShowCommandManager] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [form, setForm] = useState<CreateProfileInput>(() => createDefaultForm(connectionDefaults))
  const [formError, setFormError] = useState<string | null>(null)
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null)
  const [windowCloseConfirm, setWindowCloseConfirm] = useState<WindowCloseConfirmState | null>(null)
  const connectionDefaultsRef = useRef(connectionDefaults)
  const initializedConnectionFormKeyRef = useRef<string | null>(null)
  connectionDefaultsRef.current = connectionDefaults

  const connectionGroupOptions = useMemo(
    () =>
      collectConnectionGroups(
        folders.map((folder) => folder.name),
        profiles.map((profile) => profile.group),
        form.group
      ),
    [folders, form.group, profiles]
  )

  useEffect(() => {
    if (!isConnectionFormWindow) {
      initializedConnectionFormKeyRef.current = null
      return
    }

    // A standalone connection form is initialized after its first library
    // snapshot. Later workspace events (for example transfer updates) may
    // replace the profiles array, but must never discard text the user has
    // already entered in this window.
    if (!hasLoadedInitialSnapshot) {
      return
    }

    const formKey = `${formWindowMode}:${formWindowProfileId ?? ''}`
    if (formWindowMode === 'edit') {
      const profile = profiles.find((item) => item.id === formWindowProfileId)
      if (!profile) {
        // A standalone edit window renders once before its workspace snapshot
        // arrives. Do not briefly show a false "profile not found" error.
        if (hasLoadedInitialSnapshot) {
          setFormError(t.profileNotFound)
        }
        return
      }

      if (initializedConnectionFormKeyRef.current === formKey) {
        return
      }

      setEditingProfileId(profile.id)
      setForm(profileToForm(profile, connectionDefaultsRef.current))
      setFormError(null)
      initializedConnectionFormKeyRef.current = formKey
      return
    }

    if (initializedConnectionFormKeyRef.current === formKey) {
      return
    }

    setEditingProfileId(null)
    setForm(createDefaultForm(connectionDefaultsRef.current))
    setFormError(null)
    initializedConnectionFormKeyRef.current = formKey
  }, [formWindowMode, formWindowProfileId, hasLoadedInitialSnapshot, isConnectionFormWindow, profiles])

  const updateForm = (updater: CreateProfileInput | ((current: CreateProfileInput) => CreateProfileInput)) => {
    setForm((current) => (typeof updater === 'function' ? updater(current) : updater))
    setFormError(null)
  }

  const openCreateModal = () => {
    setEditingProfileId(null)
    setForm(createDefaultForm(connectionDefaultsRef.current))
    setFormError(null)
    setShowConnectionForm(true)
  }

  const openEditModal = (profile: ConnectionProfile) => {
    setEditingProfileId(profile.id)
    setForm(profileToForm(profile, connectionDefaultsRef.current))
    setFormError(null)
    setShowConnectionForm(true)
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
      return desktopApi.openConnectionFormWindow('edit', profile.id)
    }
    openEditModal(profile)
  }

  const closeConnectionForm = () => {
    setShowConnectionForm(false)
    setEditingProfileId(null)
    setFormError(null)
  }

  const openConnectionManager = () => {
    if (desktopApi) {
      void desktopApi.openConnectionManagerWindow()
      return
    }
    setShowConnectionManager(true)
  }

  const openCommandManager = () => {
    if (desktopApi) {
      void desktopApi.openCommandManagerWindow()
      return
    }
    setShowCommandManager(true)
  }

  const requestWindowCloseConfirmation = (isQuit: boolean, hasActiveConnections: boolean) => {
    if (desktopApi?.platform === 'darwin' && !isQuit && !hasActiveConnections) {
      void desktopApi.confirmCloseWindow('hide')
      return
    }

    setWindowCloseConfirm({ isQuit, hasActiveConnections })
  }

  const resolveWindowCloseConfirmation = (action: 'quit' | 'hide' | 'cancel') => {
    setWindowCloseConfirm(null)
    void desktopApi?.confirmCloseWindow(action)
  }

  const openCommandManagerFromSettings = () => {
    setShowSettings(false)
    openCommandManager()
  }

  const openConnectionManagerFromSettings = () => {
    setShowSettings(false)
    openConnectionManager()
  }

  return {
    closeConnectionForm,
    connectionGroupOptions,
    editingProfileId,
    form,
    formError,
    openCommandManager,
    openCommandManagerFromSettings,
    openConnectionManager,
    openConnectionManagerFromSettings,
    openCreateConnection,
    openCreateModal,
    openEditConnection,
    openEditModal,
    requestWindowCloseConfirmation,
    resolveWindowCloseConfirmation,
    setForm,
    setFormError,
    setShowCommandManager,
    setShowConnectionManager,
    setShowSettings,
    showCommandManager,
    showConnectionForm,
    showConnectionManager,
    showSettings,
    updateForm,
    windowCloseConfirm
  }
}
