import { useRef, type Dispatch, type FormEvent, type SetStateAction } from 'react'
import type {
  CommandExecutionOptions,
  ConnectionProfile,
  CreateProfileInput,
  FileTermDesktopApi,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { normalizeConnectionHost, validateConnectionHost } from '@fileterm/shared'
import { settledResultsError } from '../app/app-utils'
import type { ErrorDetails } from '../app/app-error-utils'
import { t } from '../i18n'
import { resolveSelectedTabIds, type SendScope, type SessionSendTarget } from '../features/common/session-send-targets'
import { useWorkspaceDataOps } from './use-workspace-data-ops'

type ErrorFormatter = (scope: string, error: unknown, details?: ErrorDetails) => string
type ErrorReporter = (scope: string, error: unknown, details?: ErrorDetails) => void

export type AppDataOperationsOptions = {
  desktopApi?: FileTermDesktopApi
  isCommandFormWindow: boolean
  isConnectionFormWindow: boolean
  form: CreateProfileInput
  setFormError: Dispatch<SetStateAction<string | null>>
  editingProfileId: string | null
  isBusy: boolean
  setIsBusy(value: boolean): void
  applySnapshot(snapshot: WorkspaceSnapshot): void
  closeCurrentWindow(): void
  closeConnectionForm(): void
  setError: Dispatch<SetStateAction<string | null>>
  formatError: ErrorFormatter
  onError: ErrorReporter
  waitForSshInteractionListener(): Promise<void>
  activePaneTab: WorkspaceTab | null
  visibleWorkspaceTabs: WorkspaceTab[]
  sessionSendTargets: SessionSendTarget[]
}

function reportFormError(
  setter: Dispatch<SetStateAction<string | null>>,
  formatError: ErrorFormatter,
  scope: string,
  error: unknown
) {
  console.error(`[FileTerm] ${scope}`, error)
  setter(formatError(scope, error))
}

export function useAppDataOperations({
  desktopApi,
  isCommandFormWindow,
  isConnectionFormWindow,
  form,
  setFormError,
  editingProfileId,
  isBusy,
  setIsBusy,
  applySnapshot,
  closeCurrentWindow,
  closeConnectionForm,
  setError,
  formatError,
  onError,
  waitForSshInteractionListener,
  activePaneTab,
  visibleWorkspaceTabs,
  sessionSendTargets
}: AppDataOperationsOptions) {
  const profileSaveInFlightRef = useRef(false)
  const profileTestInFlightRef = useRef(false)

  const {
    saveCommandTemplate,
    createCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    deleteCommandFolder,
    deleteCommandTemplate,
    createConnectionFolder,
    updateConnectionFolder,
    deleteConnectionFolder,
    updateConnectionOrder
  } = useWorkspaceDataOps({
    desktopApi: desktopApi ?? null,
    isCommandFormWindow,
    onApplySnapshot: applySnapshot,
    onBusyChange: setIsBusy,
    onError,
    onCloseCurrentWindow: closeCurrentWindow
  })

  const buildProfilePayload = (requireSaveFields: boolean): CreateProfileInput | null => {
    const normalizedHost = normalizeConnectionHost(form.host)
    const requiresHost = form.type !== 'serial'
    const requiresRemotePath = form.type === 'ftp' || (form.type === 'ssh' && form.deviceMode !== 'network-device')

    if (
      (requireSaveFields && (!form.name || !form.group)) ||
      (requiresHost && !normalizedHost) ||
      (requireSaveFields && requiresRemotePath && !form.remotePath) ||
      (form.type === 'serial' && !form.devicePath?.trim())
    ) {
      setFormError(requireSaveFields ? t.fillRequired : t.connectionTestFillRequired)
      return null
    }
    if (requiresHost && !validateConnectionHost(normalizedHost).valid) {
      setFormError(t.invalidHost)
      return null
    }
    if (form.type === 'ssh' && form.authType === 'privateKey' && !form.privateKeyId && !form.privateKeyPath) {
      setFormError(t.missingPrivateKeyPath)
      return null
    }

    const defaultPort = form.type === 'ftp' ? 21 : form.type === 'telnet' ? 23 : form.type === 'serial' ? 0 : 22
    return { ...form, host: normalizedHost, port: Number(form.port) || defaultPort }
  }

  const handleSaveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (isBusy || profileSaveInFlightRef.current) return
    const payload = buildProfilePayload(true)
    if (!payload) return
    if (!desktopApi) {
      setFormError(t.desktopOnlyCreate)
      return
    }

    try {
      profileSaveInFlightRef.current = true
      setIsBusy(true)
      const snapshot = editingProfileId
        ? await desktopApi.updateProfile(editingProfileId, payload)
        : await desktopApi.createProfile(payload)
      applySnapshot(snapshot)
      if (isConnectionFormWindow) {
        closeCurrentWindow()
        return
      }
      closeConnectionForm()
    } catch (error) {
      reportFormError(setFormError, formatError, '保存连接', error)
    } finally {
      profileSaveInFlightRef.current = false
      setIsBusy(false)
    }
  }

  const handleTestConnection = async (): Promise<boolean> => {
    if (isBusy || profileTestInFlightRef.current) return false
    const payload = buildProfilePayload(false)
    if (!payload) return false
    if (!desktopApi) {
      setFormError(t.desktopOnlyCreate)
      return false
    }

    try {
      profileTestInFlightRef.current = true
      setIsBusy(true)
      // Wait until the standalone form has registered its SSH interaction
      // listener so a first-time host-key prompt cannot be emitted into void.
      await waitForSshInteractionListener()
      await desktopApi.testConnection(payload, editingProfileId ?? undefined)
      setFormError(null)
      return true
    } catch (error) {
      reportFormError(setFormError, formatError, '测试连接', error)
      return false
    } finally {
      profileTestInFlightRef.current = false
      setIsBusy(false)
    }
  }

  const handleDeleteProfile = async (profileId: string) => {
    if (!desktopApi) {
      setError(t.desktopOnlyDelete)
      return false
    }
    try {
      setIsBusy(true)
      applySnapshot(await desktopApi.deleteProfile(profileId))
      return true
    } catch (error) {
      onError('删除连接', error)
      return false
    } finally {
      setIsBusy(false)
    }
  }

  const handleClearHostFingerprint = async (profile: ConnectionProfile) => {
    if (!desktopApi || profile.type !== 'ssh') return
    try {
      setIsBusy(true)
      applySnapshot(await desktopApi.clearTrustedHostFingerprint(profile.id))
      setError(null)
    } catch (error) {
      onError('清除主机指纹', error)
    } finally {
      setIsBusy(false)
    }
  }

  const executeCommandTemplate = async (
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ) => {
    if (!desktopApi) return
    try {
      setIsBusy(true)
      const targetIds = resolveSelectedTabIds(scope, activePaneTab, selectedTabIds, sessionSendTargets)
      const targetTabs = visibleWorkspaceTabs.filter((tab) => targetIds.includes(tab.id))
      // Keep command dispatch independent: one slow or failed tab must not
      // prevent the remaining selected sessions from receiving the command.
      const results = await Promise.allSettled(
        targetTabs.map((tab) => desktopApi.executeCommandTemplate(tab.id, commandId, args, options))
      )
      const failure = settledResultsError('执行命令模板', results)
      if (failure) throw failure
    } catch (error) {
      onError('执行命令模板', error)
    } finally {
      setIsBusy(false)
    }
  }

  const openLogsDirectory = () => {
    if (!desktopApi) {
      setError(t.desktopOnlyOpenLogs)
      return
    }
    void desktopApi.openLogsDirectory().catch((error) => onError(t.openLogsDirectory, error))
  }

  return {
    saveCommandTemplate,
    createCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    deleteCommandFolder,
    deleteCommandTemplate,
    createConnectionFolder,
    updateConnectionFolder,
    deleteConnectionFolder,
    updateConnectionOrder,
    handleSaveProfile,
    handleTestConnection,
    handleDeleteProfile,
    handleClearHostFingerprint,
    executeCommandTemplate,
    openLogsDirectory
  }
}
