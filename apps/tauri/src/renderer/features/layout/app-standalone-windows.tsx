import { lazy, Suspense } from 'react'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from '../commands/command-editor-modal'
import { CommandManagerModal } from '../commands/command-manager-modal'
import { ConnectionFormHost } from '../connections/connection-form-host'
import { ConnectionImportPreviewModal } from '../connections/connection-import-preview-modal'
import { ConnectionManagerModal } from '../connections/connection-manager-modal'
import { ConnectionModal } from '../connections/connection-modal'
import { CloseButton } from '../common/close-button'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { SshInteractionPortal } from './modal-portal-manager'
import { StandaloneWindowFrame } from './standalone-window-frame'
import type { AppViewModel } from './app-view-model'
import { isDarkTheme } from '../../app/theme-config'
import { t } from '../../i18n'

const FileEditorModal = lazy(() =>
  import('../files/file-editor-modal').then((module) => ({
    default: module.FileEditorModal
  }))
)

export function AppStandaloneWindows({ model }: { model: AppViewModel }) {
  const { route, shell, workspace: workspaceState, data, isWindowsDesktop } = model
  const {
    isConnectionManagerWindow,
    isCommandManagerWindow,
    isCommandFormWindow,
    isConnectionFormWindow,
    isFileEditorWindow,
    formWindowMode,
    formWindowCommandId,
    formWindowFolderId,
    formWindowCommand,
    fileEditorWindowSource,
    fileEditorWindowName
  } = route
  const {
    workspace,
    desktopApi,
    closeCurrentWindow,
    isBusy,
    connectionDefaults,
    resourceMonitoringMetrics,
    resourceMonitoringMetricOrder,
    connectionImportPlan,
    setConnectionImportPlan,
    openConnectionImportPreview,
    commitConnectionJsonPreview,
    reportError
  } = shell
  const {
    connectionGroupOptions,
    editingProfileId,
    form,
    formError,
    closeConnectionForm,
    updateForm,
    setForm,
    sshInteractionPortalProps,
    fileEditor,
    isFileEditorBusy,
    isFileEditorDirty,
    isFileEditorSaving,
    fileEditorError,
    reloadFileEditorWithEncoding,
    saveFileEditor,
    checkFileEditorDirty,
    requestFileEditorClose,
    confirmFileEditorDiscard,
    cancelFileEditorDiscard,
    isFileEditorDiscardConfirmOpen,
    openProfile,
    openCreateConnection,
    openEditConnection
  } = workspaceState
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
    handleSaveProfile,
    handleTestConnection,
    handleDeleteProfile,
    handleClearHostFingerprint
  } = data

  if (isConnectionManagerWindow) {
    return (
      <>
        <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={t.connectionManager}>
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
                void desktopApi.openProfileFromManager(profileId).catch((error) => {
                  reportError('从管理器打开连接', error)
                })
                return
              }
              void openProfile(profileId)
            }}
            onCreateFolder={createConnectionFolder}
            onDeleteFolder={deleteConnectionFolder}
            onUpdateFolder={updateConnectionFolder}
            onUpdateOrder={updateConnectionOrder}
            onImportConnections={openConnectionImportPreview}
            onExportConnections={() => {
              const request = desktopApi?.exportConnections('fileterm')
              void request?.catch((error) => reportError('导出连接', error))
            }}
          />
          {workspaceState.showConnectionForm ? (
            <ConnectionModal
              connectionDefaults={connectionDefaults}
              errorMessage={formError}
              groupOptions={connectionGroupOptions}
              fallbackResourceMonitoringMetrics={resourceMonitoringMetrics}
              fallbackResourceMonitoringMetricOrder={resourceMonitoringMetricOrder}
              mode={editingProfileId ? 'edit' : 'create'}
              form={form}
              editingProfileId={editingProfileId}
              profiles={workspace.profiles}
              hasSavedPassword={
                editingProfileId
                  ? workspace.profiles.find((profile) => profile.id === editingProfileId)?.hasSavedPassword === true
                  : false
              }
              hasSavedSudoPassword={
                editingProfileId
                  ? workspace.profiles.find((profile) => profile.id === editingProfileId)?.hasSavedSudoPassword === true
                  : false
              }
              hasSavedSuPassword={
                editingProfileId
                  ? workspace.profiles.find((profile) => profile.id === editingProfileId)?.hasSavedSuPassword === true
                  : false
              }
              isSubmitting={isBusy}
              setForm={updateForm}
              onClearHostFingerprint={() => {
                const editingProfile = editingProfileId
                  ? (workspace.profiles.find((profile) => profile.id === editingProfileId) ?? null)
                  : null
                if (editingProfile) {
                  void handleClearHostFingerprint(editingProfile)
                  setForm((previous) => ({ ...previous, trustedHostFingerprint: '' }))
                }
              }}
              onTestConnection={handleTestConnection}
              onSubmit={handleSaveProfile}
              onClose={closeConnectionForm}
            />
          ) : null}
        </StandaloneWindowFrame>
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

  if (isCommandManagerWindow) {
    return (
      <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={t.commandManager}>
        <CommandManagerModal
          commandFolders={workspace.commandFolders || []}
          commandTemplates={workspace.commandTemplates || []}
          standalone
          onClose={closeCurrentWindow}
          onCreateFolder={createCommandFolder}
          onDeleteFolder={deleteCommandFolder}
          onUpdateFolder={updateCommandFolder}
          onUpdateOrder={updateCommandOrder}
          onCreateCommand={(input) => saveCommandTemplate(null, input)}
          onUpdateCommand={(commandId, input) => saveCommandTemplate(commandId, input)}
          onDeleteCommand={deleteCommandTemplate}
        />
      </StandaloneWindowFrame>
    )
  }

  if (isCommandFormWindow) {
    const editingCommand =
      formWindowMode === 'edit'
        ? (workspace.commandTemplates.find((item) => item.id === formWindowCommandId) ?? null)
        : null

    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={editingCommand ? t.commandEdit : t.commandCreate}
      >
        <CommandEditorModal
          folders={workspace.commandFolders || []}
          initialValue={
            editingCommand
              ? toCommandTemplateInput(editingCommand)
              : {
                  ...emptyCommandForm,
                  command: formWindowCommand,
                  parentId: formWindowFolderId || undefined
                }
          }
          mode={editingCommand ? 'edit' : formWindowMode}
          isSubmitting={isBusy}
          standalone
          onClose={closeCurrentWindow}
          onSubmit={(input) => saveCommandTemplate(editingCommand?.id ?? null, input)}
        />
      </StandaloneWindowFrame>
    )
  }

  if (isConnectionFormWindow) {
    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={editingProfileId ? t.editConnection : t.newConnection}
      >
        <ConnectionFormHost
          connectionDefaults={connectionDefaults}
          editingProfileId={editingProfileId}
          errorMessage={formError}
          fallbackResourceMonitoringMetrics={resourceMonitoringMetrics}
          fallbackResourceMonitoringMetricOrder={resourceMonitoringMetricOrder}
          groupOptions={connectionGroupOptions}
          mode={editingProfileId ? 'edit' : formWindowMode}
          form={form}
          isSubmitting={isBusy}
          profiles={workspace.profiles}
          setForm={updateForm}
          onClearHostFingerprint={(profile) => {
            void handleClearHostFingerprint(profile)
          }}
          standalone
          onTestConnection={handleTestConnection}
          onSubmit={handleSaveProfile}
          onClose={closeCurrentWindow}
        />
        <SshInteractionPortal {...sshInteractionPortalProps} />
      </StandaloneWindowFrame>
    )
  }

  if (isFileEditorWindow && fileEditor) {
    return (
      <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={fileEditor.name}>
        <Suspense fallback={<div aria-busy="true" className="standalone-shell file-editor-window" />}>
          <FileEditorModal
            errorMessage={fileEditorError}
            file={fileEditor}
            isBusy={isFileEditorBusy}
            isDirty={isFileEditorDirty}
            isSaving={isFileEditorSaving}
            onClose={requestFileEditorClose}
            onDraftChange={checkFileEditorDirty}
            onReloadWithEncoding={(encoding) => {
              void reloadFileEditorWithEncoding(encoding)
            }}
            onSave={saveFileEditor}
            standalone
            themeMode={shell.themeMode}
          />
        </Suspense>
        {isFileEditorDiscardConfirmOpen ? (
          <ConfirmActionDialog
            confirmLabel={t.fileEditorDiscardChanges}
            description={t.fileEditorDiscardChangesDescription}
            onClose={cancelFileEditorDiscard}
            onConfirm={confirmFileEditorDiscard}
            title={t.fileEditorDiscardChangesTitle}
          />
        ) : null}
      </StandaloneWindowFrame>
    )
  }

  if (isFileEditorWindow) {
    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={fileEditorWindowName ?? t.appTitle}
      >
        <div aria-busy={!fileEditorError} className="standalone-shell file-editor-window">
          <div
            className={`modal-card file-editor-modal ${isDarkTheme(shell.themeMode) ? 'file-editor-modal--dark' : ''} standalone`}
          >
            <div className="modal-header" data-tauri-drag-region="deep">
              <div className="file-editor-title">
                <span>{fileEditorWindowSource === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
                <strong>{fileEditorWindowName ?? ''}</strong>
              </div>
              <div className="file-editor-header-actions">
                <CloseButton onClick={closeCurrentWindow} />
              </div>
            </div>
            {fileEditorError ? <div className="modal-error">{fileEditorError}</div> : null}
          </div>
        </div>
      </StandaloneWindowFrame>
    )
  }

  return null
}
