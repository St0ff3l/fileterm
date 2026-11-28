import './web-window-overlay-host.css'
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import type { ConnectionProfile, CreateProfileInput, FileContentSnapshot, RemoteSystemPlatform } from '@fileterm/core'
import { defaultForm, profileToForm } from '../../app/app-data'
import { isDarkTheme } from '../../app/theme-config'
import { invalidateChangedConnectionHostTrust } from '../../hooks/connection-host-trust'
import { t } from '../../i18n'
import { APP_EVENT, onAppEvent, type WebWindowRequest } from '../../lib/app-events'
import { CloseButton } from '../common/close-button'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from '../commands/command-editor-modal'
import { CommandManagerModal } from '../commands/command-manager-modal'
import { ConnectionFormHost } from '../connections/connection-form-host'
import { ConnectionImportPreviewModal } from '../connections/connection-import-preview-modal'
import { ConnectionManagerModal } from '../connections/connection-manager-modal'
import { ProxyFormHost } from '../proxies/proxy-form-host'
import { TunnelFormHost } from '../tunnels/tunnel-form-host'
import type { AppViewModel } from './app-view-model'

const FileEditorModal = lazy(() =>
  import('../files/file-editor-modal').then((module) => ({
    default: module.FileEditorModal
  }))
)

type OverlayStackEntry = {
  id: number
  request: WebWindowRequest
}

export function WebWindowOverlayHost({ model }: { model: AppViewModel }) {
  const [stack, setStack] = useState<OverlayStackEntry[]>([])
  const nextIdRef = useRef(1)
  const topEntry = stack.at(-1) ?? null

  const closeTop = useCallback(() => {
    setStack((current) => current.slice(0, -1))
  }, [])

  const closeAll = useCallback(() => {
    setStack([])
  }, [])

  useEffect(() => {
    const offOpen = onAppEvent(APP_EVENT.webOpenWindow, (request) => {
      setStack((current) => {
        const existingIndex = current.findIndex(
          (entry) => entry.request.kind === request.kind && request.kind !== 'file-editor'
        )
        if (request.kind === 'connection-manager' || request.kind === 'command-manager') {
          if (existingIndex >= 0) {
            return [
              ...current.slice(0, existingIndex),
              ...current.slice(existingIndex + 1),
              { id: nextIdRef.current++, request }
            ]
          }
        }
        return [...current, { id: nextIdRef.current++, request }]
      })
    })
    const offClose = onAppEvent(APP_EVENT.webCloseWindow, () => {
      closeTop()
    })
    return () => {
      offOpen()
      offClose()
    }
  }, [closeTop])

  useEffect(() => {
    if (!topEntry) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && topEntry.request.kind !== 'file-editor') {
        closeTop()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [closeTop, topEntry])

  if (!stack.length) {
    return null
  }

  return (
    <>
      {stack.map((entry, index) => (
        <div
          key={entry.id}
          className="web-overlay-backdrop"
          style={{ zIndex: 80 + index }}
          role="dialog"
          aria-modal="true"
        >
          <div
            className={`web-overlay-frame ${entry.request.kind === 'file-editor' ? 'web-overlay-frame--editor' : ''}`}
          >
            <WebOverlayContent model={model} request={entry.request} onClose={closeTop} onCloseAll={closeAll} />
          </div>
        </div>
      ))}
    </>
  )
}

function WebOverlayContent({
  model,
  request,
  onClose,
  onCloseAll
}: {
  model: AppViewModel
  request: WebWindowRequest
  onClose(): void
  onCloseAll(): void
}) {
  const { shell, workspace: workspaceState, data } = model
  const {
    workspace,
    desktopApi,
    isBusy,
    connectionImportPlan,
    setConnectionImportPlan,
    openConnectionImportPreview,
    commitConnectionJsonPreview,
    reportError,
    applySnapshot
  } = shell
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
    updateConnectionOrder,
    handleDeleteProfile
  } = data

  if (request.kind === 'connection-manager') {
    return (
      <>
        <ConnectionManagerModal
          profiles={workspace.profiles}
          folders={workspace.folders || []}
          standalone
          onClose={onClose}
          onCreate={() => void desktopApi?.openConnectionFormWindow('create')}
          onDeleteProfile={handleDeleteProfile}
          onEditProfile={(profile) => {
            void desktopApi?.openConnectionFormWindow('edit', profile.id)
          }}
          onOpenProfile={(profileId) => {
            onCloseAll()
            void workspaceState.openProfile(profileId)
          }}
          onCreateFolder={createConnectionFolder}
          onDeleteFolder={deleteConnectionFolder}
          onUpdateFolder={updateConnectionFolder}
          onUpdateOrder={updateConnectionOrder}
          onImportConnections={openConnectionImportPreview}
          onExportConnections={() => {
            const exportRequest = desktopApi?.exportConnections('fileterm')
            void exportRequest?.catch((error) => reportError('导出连接', error))
          }}
        />
        {connectionImportPlan ? (
          <ConnectionImportPreviewModal
            plan={connectionImportPlan}
            onClose={() => setConnectionImportPlan(null)}
            onCommit={commitConnectionJsonPreview}
          />
        ) : null}
      </>
    )
  }

  if (request.kind === 'command-manager') {
    return (
      <CommandManagerModal
        commandFolders={workspace.commandFolders || []}
        commandTemplates={workspace.commandTemplates || []}
        standalone
        onClose={onClose}
        onCreateFolder={createCommandFolder}
        onDeleteFolder={deleteCommandFolder}
        onUpdateFolder={updateCommandFolder}
        onUpdateOrder={updateCommandOrder}
        onCreateCommand={(input) => saveCommandTemplate(null, input)}
        onUpdateCommand={(commandId, input) => saveCommandTemplate(commandId, input)}
        onDeleteCommand={deleteCommandTemplate}
      />
    )
  }

  if (request.kind === 'command-form') {
    const editingCommand =
      request.mode === 'edit'
        ? (workspace.commandTemplates.find((item) => item.id === request.commandId) ?? null)
        : null

    return (
      <CommandEditorModal
        folders={workspace.commandFolders || []}
        initialValue={
          editingCommand
            ? toCommandTemplateInput(editingCommand)
            : {
                ...emptyCommandForm,
                command: request.command ?? '',
                parentId: request.folderId || undefined
              }
        }
        mode={editingCommand ? 'edit' : request.mode}
        isSubmitting={isBusy}
        standalone
        onClose={onClose}
        onSubmit={async (input) => {
          await saveCommandTemplate(editingCommand?.id ?? null, input)
          onClose()
        }}
      />
    )
  }

  if (request.kind === 'connection-form') {
    return (
      <WebConnectionFormOverlay model={model} mode={request.mode} profileId={request.profileId} onClose={onClose} />
    )
  }

  if (request.kind === 'proxy-form') {
    return (
      <ProxyFormHost
        mode={request.mode}
        proxyId={request.profileId ?? null}
        initialType={request.proxyType ?? 'socks5'}
        standalone
        onClose={onClose}
      />
    )
  }

  if (request.kind === 'tunnel-form') {
    return (
      <TunnelFormHost
        mode={request.mode}
        tunnelId={request.profileId ?? null}
        initialType={request.tunnelType ?? 'ssh'}
        standalone
        onClose={onClose}
      />
    )
  }

  return (
    <WebFileEditorOverlay
      desktopApi={desktopApi}
      remotePlatform={
        workspace.sessions[request.tabId ?? '']?.systemMetrics?.platform as RemoteSystemPlatform | undefined
      }
      request={request}
      themeMode={shell.themeMode}
      formatError={shell.formatAppError}
      onApplySnapshot={applySnapshot}
      onClose={onClose}
    />
  )
}

function WebConnectionFormOverlay({
  model,
  mode,
  profileId,
  onClose
}: {
  model: AppViewModel
  mode: 'create' | 'edit'
  profileId?: string
  onClose(): void
}) {
  const { shell, workspace: workspaceState, data } = model
  const {
    workspace,
    desktopApi,
    connectionDefaults,
    transferSidebarCollapsedByDefault,
    resourceMonitoringMetrics,
    resourceMonitoringMetricOrder,
    applySnapshot,
    formatAppError
  } = shell
  const { connectionGroupOptions, waitForSshInteractionListener } = workspaceState
  const { handleClearHostFingerprint } = data

  const editingProfile: ConnectionProfile | null = useMemo(
    () => (mode === 'edit' && profileId ? (workspace.profiles.find((p) => p.id === profileId) ?? null) : null),
    [mode, profileId, workspace.profiles]
  )

  const [form, setFormState] = useState<CreateProfileInput>(() =>
    editingProfile
      ? profileToForm(editingProfile, connectionDefaults)
      : { ...defaultForm, ...connectionDefaults, group: t.defaultGroup }
  )
  const [formError, setFormError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const setForm = useCallback((updater: CreateProfileInput | ((current: CreateProfileInput) => CreateProfileInput)) => {
    setFormState((current) =>
      invalidateChangedConnectionHostTrust(current, typeof updater === 'function' ? updater(current) : updater)
    )
    setFormError(null)
  }, [])

  const handleTestConnection = useCallback(async () => {
    if (!desktopApi) return false
    setIsSubmitting(true)
    setFormError(null)
    try {
      await waitForSshInteractionListener()
      await desktopApi.testConnection(form, editingProfile?.id)
      return true
    } catch (error) {
      setFormError(formatAppError('测试连接', error))
      return false
    } finally {
      setIsSubmitting(false)
    }
  }, [desktopApi, editingProfile?.id, form, formatAppError, waitForSshInteractionListener])

  const handleSubmit = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      if (!desktopApi || isSubmitting) return
      setIsSubmitting(true)
      setFormError(null)
      const saveRequest = editingProfile
        ? desktopApi.updateProfile(editingProfile.id, form)
        : desktopApi.createProfile(form)
      void saveRequest
        .then((snapshot) => {
          applySnapshot(snapshot)
          onClose()
        })
        .catch((error) => {
          setFormError(formatAppError(editingProfile ? '更新连接' : '创建连接', error))
        })
        .finally(() => {
          setIsSubmitting(false)
        })
    },
    [applySnapshot, desktopApi, editingProfile, form, formatAppError, isSubmitting, onClose]
  )

  return (
    <ConnectionFormHost
      connectionDefaults={connectionDefaults}
      defaultTransferSidebarCollapsedByDefault={transferSidebarCollapsedByDefault}
      editingProfileId={editingProfile?.id ?? null}
      errorMessage={formError}
      fallbackResourceMonitoringMetrics={resourceMonitoringMetrics}
      fallbackResourceMonitoringMetricOrder={resourceMonitoringMetricOrder}
      groupOptions={connectionGroupOptions}
      mode={editingProfile ? 'edit' : mode}
      form={form}
      isSubmitting={isSubmitting}
      profiles={workspace.profiles}
      setForm={setForm}
      onClearHostFingerprint={(profile) => {
        void handleClearHostFingerprint(profile)
      }}
      standalone
      onTestConnection={handleTestConnection}
      onSubmit={handleSubmit}
      onClose={onClose}
    />
  )
}

function WebFileEditorOverlay({
  desktopApi,
  remotePlatform,
  request,
  themeMode,
  formatError,
  onApplySnapshot,
  onClose
}: {
  desktopApi: AppViewModel['shell']['desktopApi']
  remotePlatform?: RemoteSystemPlatform
  request: Extract<WebWindowRequest, { kind: 'file-editor' }>
  themeMode: AppViewModel['shell']['themeMode']
  formatError: AppViewModel['shell']['formatAppError']
  onApplySnapshot: AppViewModel['shell']['applySnapshot']
  onClose(): void
}) {
  const [file, setFile] = useState<FileContentSnapshot | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [isSaving, setIsSaving] = useState(false)
  const [isDirty, setIsDirty] = useState(false)
  const [isDiscardOpen, setIsDiscardOpen] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)

  const loadWithEncoding = useCallback(
    async (encoding: string) => {
      if (!desktopApi) return
      setIsLoading(true)
      try {
        const content =
          request.source === 'local'
            ? await desktopApi.readLocalFile(request.path, encoding)
            : await desktopApi.readRemoteFile(request.tabId!, request.path, encoding)
        setFile({
          source: request.source,
          path: request.path,
          name: request.name,
          tabId: request.tabId,
          encoding,
          content
        })
        setErrorMessage(null)
        setIsDirty(false)
      } catch (error) {
        setErrorMessage(formatError('打开文件', error, { targetPath: request.path }))
      } finally {
        setIsLoading(false)
      }
    },
    [desktopApi, formatError, request.name, request.path, request.source, request.tabId]
  )

  useEffect(() => {
    void loadWithEncoding(request.encoding ?? 'utf-8')
  }, [loadWithEncoding, request.encoding])

  const handleSave = useCallback(
    async (content: string, encoding: string) => {
      if (!desktopApi || !file || isSaving) return
      setIsSaving(true)
      try {
        if (file.source === 'local') {
          await desktopApi.writeLocalFile(file.path, content, encoding)
        } else if (file.tabId) {
          onApplySnapshot(await desktopApi.writeRemoteFile(file.tabId, file.path, content, encoding))
        }
        setFile({ ...file, content, encoding })
        setErrorMessage(null)
        setIsDirty(false)
      } catch (error) {
        setErrorMessage(formatError('保存文件', error, { targetPath: file.path }))
      } finally {
        setIsSaving(false)
      }
    },
    [desktopApi, file, formatError, isSaving, onApplySnapshot]
  )

  const requestClose = useCallback(() => {
    if (isDirty) {
      setIsDiscardOpen(true)
      return
    }
    onClose()
  }, [isDirty, onClose])

  if (!file) {
    return (
      <div
        aria-busy={isLoading}
        className={`modal-card file-editor-modal ${isDarkTheme(themeMode) ? 'file-editor-modal--dark' : ''} standalone`}
      >
        <div className="modal-header">
          <div className="file-editor-title">
            <span>{request.source === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
            <strong>{request.name}</strong>
          </div>
          <div className="file-editor-header-actions">
            <CloseButton onClick={onClose} />
          </div>
        </div>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
      </div>
    )
  }

  return (
    <>
      <Suspense fallback={<div aria-busy="true" className="standalone-shell file-editor-window" />}>
        <FileEditorModal
          errorMessage={errorMessage}
          file={file}
          remotePlatform={remotePlatform}
          isBusy={isLoading || isSaving}
          isDirty={isDirty}
          isSaving={isSaving}
          onClose={requestClose}
          onDraftChange={(content, encoding) => {
            setIsDirty(content !== file.content || encoding !== (file.encoding ?? 'utf-8'))
          }}
          onReloadWithEncoding={(encoding) => void loadWithEncoding(encoding)}
          onSave={handleSave}
          standalone
          themeMode={themeMode}
        />
      </Suspense>
      {isDiscardOpen ? (
        <ConfirmActionDialog
          confirmLabel={t.fileEditorDiscardChanges}
          description={t.fileEditorDiscardChangesDescription}
          onClose={() => setIsDiscardOpen(false)}
          onConfirm={onClose}
          title={t.fileEditorDiscardChangesTitle}
        />
      ) : null}
    </>
  )
}
