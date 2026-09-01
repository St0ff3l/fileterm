import { useMemo } from 'react'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { ConnectionImportPreviewModal } from '../connections/connection-import-preview-modal'
import { SelectionControl } from '../common/selection-control'
import { ModalPortalManager, type FileActionModalBinding } from './modal-portal-manager'
import { SessionLockScreen } from '../security/session-lock-screen'
import type { AppViewModel } from './app-view-model'
import { setLocale, t } from '../../i18n'

function actionApprovalSourceLabel(source: 'cli' | 'mcp' | 'ai-copilot') {
  if (source === 'cli') return t.sessionSourceCli
  if (source === 'mcp') return t.sessionSourceMcp
  return 'Copilot'
}

export function AppModalPortals({ model }: { model: AppViewModel }) {
  const { route, shell, workspace: workspaceState, data } = model
  const { isMainWorkspaceWindow } = route
  const {
    workspace,
    desktopApi,
    isBusy,
    themeMode,
    themeConfig,
    customThemes,
    locale,
    setLocaleState,
    connectionDefaults,
    connectionImportPlan,
    setConnectionImportPlan,
    commitConnectionJsonPreview,
    actionApprovalRequests,
    resolvingActionApprovalId,
    riskAcknowledgedRequestId,
    setRiskAcknowledgedRequestId,
    resolveActionApproval,
    sessionSecurity,
    setThemeConfig,
    setCustomThemes,
    settingsInitialTab
  } = shell
  const {
    connectionGroupOptions,
    editingProfileId,
    form,
    formError,
    updateForm,
    closeConnectionForm,
    showCommandManager,
    showConnectionForm,
    showConnectionManager,
    showSettings,
    setShowCommandManager,
    setShowConnectionManager,
    setShowSettings,
    openCreateConnection,
    openEditConnection,
    openProfile,
    openConnectionManagerFromSettings,
    openCommandManagerFromSettings,
    windowCloseConfirm,
    resolveWindowCloseConfirmation,
    shortcutCloseConfirm,
    dismissShortcutCloseConfirm,
    confirmShortcutClose,
    fileEditor,
    fileEditorError,
    isFileEditorBusy,
    isFileEditorDirty,
    isFileEditorSaving,
    closeFileEditor,
    checkFileEditorDirty,
    reloadFileEditorWithEncoding,
    saveFileEditor,
    tabContextMenu,
    closeTabContextMenu,
    handleTabContextAction,
    sshInteractionPortalProps,
    backupPasswordRequest,
    backupPasswordError,
    isBackupPasswordResolving,
    cancelBackupPassword,
    submitBackupPassword,
    sudoPasswordRequest,
    sudoPasswordError,
    isSudoPasswordResolving,
    cancelSudoPassword,
    submitSudoPassword,
    permissionDialog,
    permissionDialogError,
    isPermissionSubmitting,
    dismissPermissionDialog,
    handleSubmitPermissions,
    rootAccessDialog,
    rootAccessDialogError,
    isRootAccessSubmitting,
    dismissRootAccessDialog,
    handleConfirmRootAccess,
    localNetworkCredentialsDialog,
    localNetworkCredentialsDialogError,
    isLocalNetworkCredentialsSubmitting,
    dismissLocalNetworkCredentialsDialog,
    handleSubmitLocalNetworkCredentials,
    localNetworkShareDialog,
    localNetworkShareDialogError,
    dismissLocalNetworkShareDialog,
    handleSubmitLocalNetworkShare,
    changeLocalNetworkCredentials,
    fileActionDialog,
    fileActionError,
    isFileActionSubmitting,
    dismissFileActionDialog,
    handleSubmitFileAction,
    localTabs,
    visibleWorkspaceTabs
  } = workspaceState
  const {
    saveCommandTemplate,
    createCommandFolder,
    deleteCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    deleteCommandTemplate,
    createConnectionFolder,
    deleteConnectionFolder,
    updateConnectionFolder,
    updateConnectionOrder,
    handleDeleteProfile,
    handleClearHostFingerprint,
    openLogsDirectory
  } = data

  const fileActionProps = useMemo<FileActionModalBinding>(() => {
    if (!fileActionDialog) return null
    if (fileActionDialog.kind === 'delete') {
      return {
        kind: 'delete',
        props: {
          confirmLabel: t.delete,
          description:
            fileActionDialog.targets.length > 1
              ? `${t.deleteConfirmPrefix}${fileActionDialog.targets.length} ${t.itemsSuffix}${t.deleteConfirmSuffix}`
              : `${t.deleteConfirmPrefix}${fileActionDialog.targets[0]?.name ?? ''}${t.deleteConfirmSuffix}`,
          errorMessage: fileActionError,
          isSubmitting: isFileActionSubmitting,
          onClose: dismissFileActionDialog,
          onConfirm: () => void handleSubmitFileAction(''),
          title: t.delete
        }
      }
    }
    return {
      kind: 'action',
      props: {
        confirmLabel: t.confirm,
        errorMessage: fileActionError,
        hint: fileActionDialog.kind === 'new-file' ? t.newFileExtensionHint : undefined,
        initialValue: fileActionDialog.kind === 'rename' ? fileActionDialog.target.name : '',
        isSubmitting: isFileActionSubmitting,
        inputLabel: t.fileName,
        inputPlaceholder: fileActionDialog.kind === 'new-folder' ? t.folderName : t.fileName,
        onClose: dismissFileActionDialog,
        onConfirm: (value: string) => void handleSubmitFileAction(value),
        title:
          fileActionDialog.kind === 'new-folder'
            ? t.newFolder
            : fileActionDialog.kind === 'new-file'
              ? t.newFile
              : t.rename
      }
    }
  }, [dismissFileActionDialog, fileActionDialog, fileActionError, handleSubmitFileAction, isFileActionSubmitting])

  const windowCloseConfirmProps = windowCloseConfirm
    ? {
        backdropClassName: sessionSecurity.isLocked ? 'session-lock-close-confirm-backdrop' : undefined,
        confirmLabel: t.closeConfirmQuit,
        confirmVariant: 'danger' as const,
        description: (
          <>
            {windowCloseConfirm.hasActiveConnections ? (
              <div className="confirm-action-dialog__warning">{t.closeConfirmActiveWarn}</div>
            ) : windowCloseConfirm.isQuit ? (
              <div>{t.closeConfirmQuitMsg}</div>
            ) : null}
            {!windowCloseConfirm.isQuit ? <div>{t.closeConfirmWindowsMsg}</div> : null}
          </>
        ),
        extraActions: !windowCloseConfirm.isQuit ? (
          <button
            className="confirm-action-dialog__button confirm-action-dialog__button--primary"
            onClick={() => resolveWindowCloseConfirmation('hide')}
            type="button"
          >
            {t.closeConfirmHide}
          </button>
        ) : null,
        initialFocus: 'none' as const,
        onClose: () => resolveWindowCloseConfirmation('cancel'),
        onConfirm: () => resolveWindowCloseConfirmation('quit'),
        title: t.closeConfirmTitle
      }
    : null

  return (
    <>
      {connectionImportPlan ? (
        <ConnectionImportPlanPortal
          plan={connectionImportPlan}
          onClose={() => setConnectionImportPlan(null)}
          onCommit={commitConnectionJsonPreview}
        />
      ) : null}
      <ModalPortalManager
        commandManager={
          showCommandManager
            ? {
                commandFolders: workspace.commandFolders || [],
                commandTemplates: workspace.commandTemplates || [],
                onClose: () => setShowCommandManager(false),
                onCreateFolder: createCommandFolder,
                onDeleteFolder: deleteCommandFolder,
                onUpdateFolder: updateCommandFolder,
                onUpdateOrder: updateCommandOrder,
                onCreateCommand: (input) => saveCommandTemplate(null, input),
                onUpdateCommand: (commandId, input) => saveCommandTemplate(commandId, input),
                onDeleteCommand: deleteCommandTemplate
              }
            : null
        }
        connectionForm={
          showConnectionForm
            ? {
                editingProfileId,
                errorMessage: formError,
                connectionDefaults,
                groupOptions: connectionGroupOptions,
                mode: editingProfileId ? 'edit' : 'create',
                form,
                isSubmitting: isBusy,
                profiles: workspace.profiles,
                setForm: updateForm,
                onClearHostFingerprint: (profile) => void handleClearHostFingerprint(profile),
                onTestConnection: data.handleTestConnection,
                onSubmit: data.handleSaveProfile,
                onClose: closeConnectionForm
              }
            : null
        }
        connectionManager={
          showConnectionManager
            ? {
                profiles: workspace.profiles,
                folders: workspace.folders || [],
                onClose: () => setShowConnectionManager(false),
                onCreate: () => {
                  setShowConnectionManager(false)
                  openCreateConnection()
                },
                onDeleteProfile: handleDeleteProfile,
                onEditProfile: (profile) => {
                  setShowConnectionManager(false)
                  openEditConnection(profile)
                },
                onOpenProfile: (profileId) => {
                  setShowConnectionManager(false)
                  void openProfile(profileId)
                },
                onCreateFolder: createConnectionFolder,
                onDeleteFolder: deleteConnectionFolder,
                onUpdateFolder: updateConnectionFolder,
                onUpdateOrder: updateConnectionOrder,
                onImportConnections: shell.openConnectionImportPreview,
                onExportConnections: () => {
                  const request = desktopApi?.exportConnections('fileterm')
                  void request?.catch((error) => shell.reportError('导出连接', error))
                }
              }
            : null
        }
        fileAction={fileActionProps}
        fileEditor={
          fileEditor
            ? {
                errorMessage: fileEditorError,
                file: fileEditor,
                isBusy: isFileEditorBusy,
                isDirty: isFileEditorDirty,
                isSaving: isFileEditorSaving,
                onClose: closeFileEditor,
                onDraftChange: checkFileEditorDirty,
                onReloadWithEncoding: (encoding) => void reloadFileEditorWithEncoding(encoding),
                onSave: saveFileEditor,
                themeMode
              }
            : null
        }
        filePermission={
          permissionDialog
            ? {
                errorMessage: permissionDialogError,
                fileName: permissionDialog.target.name,
                fileType: permissionDialog.target.type,
                initialPermission: permissionDialog.target.permission,
                isSubmitting: isPermissionSubmitting,
                onClose: dismissPermissionDialog,
                onSubmit: (options) => void handleSubmitPermissions(options),
                ownerGroup: permissionDialog.target.ownerGroup,
                supportsRecursive: permissionDialog.supportsRecursive,
                targetPath: permissionDialog.target.path
              }
            : null
        }
        rootAccess={
          rootAccessDialog
            ? {
                defaultRootAccessMethod: rootAccessDialog.rootAccessMethod,
                defaultSshUser: rootAccessDialog.sshUser,
                defaultSudoUser: rootAccessDialog.sudoUser,
                errorMessage: rootAccessDialogError,
                hasSavedSudoPassword: rootAccessDialog.hasSavedSudoPassword,
                hasSavedSuPassword: rootAccessDialog.hasSavedSuPassword,
                isSubmitting: isRootAccessSubmitting,
                onClose: dismissRootAccessDialog,
                onSubmit: handleConfirmRootAccess
              }
            : null
        }
        smbCredentials={
          localNetworkCredentialsDialog
            ? {
                errorMessage: localNetworkCredentialsDialogError,
                isSubmitting: isLocalNetworkCredentialsSubmitting,
                path: localNetworkCredentialsDialog.path,
                onCancel: dismissLocalNetworkCredentialsDialog,
                onSubmit: handleSubmitLocalNetworkCredentials
              }
            : null
        }
        smbSharePicker={
          localNetworkShareDialog
            ? {
                errorMessage: localNetworkShareDialogError,
                isSubmitting: isLocalNetworkCredentialsSubmitting,
                path: localNetworkShareDialog.path,
                shares: localNetworkShareDialog.shares,
                onCancel: dismissLocalNetworkShareDialog,
                onChangeCredentials: changeLocalNetworkCredentials,
                onSubmit: handleSubmitLocalNetworkShare
              }
            : null
        }
        settings={
          showSettings
            ? {
                theme: themeMode,
                themeConfig,
                customThemes,
                onSetTheme: shell.handleSetTheme,
                onSetThemeConfig: setThemeConfig,
                onSetCustomThemes: setCustomThemes,
                locale,
                onSetLocale: (nextLocale) => {
                  setLocale(nextLocale)
                  setLocaleState(nextLocale)
                },
                onOpenCommandManager: openCommandManagerFromSettings,
                onOpenConnectionManager: openConnectionManagerFromSettings,
                onOpenLogsDirectory: openLogsDirectory,
                onLaunchLocalAgent: workspaceState.launchLocalAgent,
                initialTab: settingsInitialTab,
                onClose: () => setShowSettings(false)
              }
            : null
        }
        shortcutCloseConfirm={
          shortcutCloseConfirm
            ? {
                confirmLabel: t.closeShortcutCloseTab,
                description: (shortcutCloseConfirm.variant === 'connecting'
                  ? t.closeShortcutConnectingDescription
                  : shortcutCloseConfirm.variant === 'active-session'
                    ? t.closeShortcutActiveDescription
                    : t.closeShortcutLastActiveDescription
                ).replace('{name}', shortcutCloseConfirm.title),
                initialFocus: 'none' as const,
                isSubmitting: isBusy,
                onClose: dismissShortcutCloseConfirm,
                onConfirm: () => void confirmShortcutClose(),
                title:
                  shortcutCloseConfirm.variant === 'connecting'
                    ? t.closeShortcutConnectingTitle
                    : shortcutCloseConfirm.variant === 'active-session'
                      ? t.closeShortcutActiveTitle
                      : t.closeShortcutLastActiveTitle
              }
            : null
        }
        {...sshInteractionPortalProps}
        backupPassword={
          backupPasswordRequest
            ? {
                request: backupPasswordRequest,
                errorMessage: backupPasswordError,
                isSubmitting: isBackupPasswordResolving,
                onCancel: () => void cancelBackupPassword(),
                onSubmit: (value) => void submitBackupPassword(value)
              }
            : null
        }
        sudoPasswordPrompt={
          sudoPasswordRequest
            ? {
                request: sudoPasswordRequest,
                errorMessage: sudoPasswordError,
                isSubmitting: isSudoPasswordResolving,
                onCancel: () => void cancelSudoPassword(),
                onSubmit: (value, save) => void submitSudoPassword(value, save)
              }
            : null
        }
        tabContextMenu={
          tabContextMenu
            ? {
                canConnectAll: visibleWorkspaceTabs.some(
                  (tab) => tab.status !== 'connected' && tab.status !== 'connecting'
                ),
                canCloseAll: localTabs.length + visibleWorkspaceTabs.length > 0,
                canCloseCurrent:
                  tabContextMenu.target.kind === 'session' ? true : localTabs.length + visibleWorkspaceTabs.length > 1,
                canCloseOthers: localTabs.length + visibleWorkspaceTabs.length > 1,
                canSaveSessionLog:
                  tabContextMenu.target.kind === 'session' && tabContextMenu.target.sessionType !== 'ftp',
                isSessionTab: tabContextMenu.target.kind === 'session',
                onAction: (action) => void handleTabContextAction(action),
                onClose: closeTabContextMenu,
                position: { x: tabContextMenu.x, y: tabContextMenu.y },
                tabStatus: tabContextMenu.target.kind === 'session' ? tabContextMenu.target.status : null
              }
            : null
        }
        windowCloseConfirm={windowCloseConfirmProps}
      />
      {isMainWorkspaceWindow && actionApprovalRequests[0] ? (
        <ConfirmActionDialog
          className="external-operation-confirmation"
          confirmLabel={t.confirm}
          confirmVariant={actionApprovalRequests[0].destructive ? 'danger' : 'primary'}
          description={
            <div className="external-operation-confirmation__content">
              <p className="external-operation-confirmation__summary">{actionApprovalRequests[0].summary}</p>
              <div className="external-operation-confirmation__field">
                <span className="external-operation-confirmation__label">{t.sessionSource}</span>
                <div className="external-operation-confirmation__source-row">
                  <span className={`external-operation-confirmation__source is-${actionApprovalRequests[0].source}`}>
                    {actionApprovalSourceLabel(actionApprovalRequests[0].source)}
                  </span>
                </div>
              </div>
              {actionApprovalRequests[0].target ? (
                <div className="external-operation-confirmation__field">
                  <span className="external-operation-confirmation__label">{t.actionApprovalTarget}</span>
                  <div className="external-operation-confirmation__value">{actionApprovalRequests[0].target}</div>
                </div>
              ) : null}
              {actionApprovalRequests[0].details ? (
                <div className="external-operation-confirmation__field">
                  <span className="external-operation-confirmation__label">{t.actionApprovalCommand}</span>
                  <pre className="external-operation-confirmation__command">{actionApprovalRequests[0].details}</pre>
                </div>
              ) : null}
              {actionApprovalRequests[0].requiresRiskAcknowledgement ? (
                <label className="confirm-action-dialog__warning">
                  <SelectionControl
                    checked={riskAcknowledgedRequestId === actionApprovalRequests[0].requestId}
                    disabled={resolvingActionApprovalId === actionApprovalRequests[0].requestId}
                    onChange={(event) =>
                      setRiskAcknowledgedRequestId(event.target.checked ? actionApprovalRequests[0].requestId : null)
                    }
                    type="checkbox"
                  />
                  <span>{t.actionApprovalRiskAcknowledgement}</span>
                </label>
              ) : null}
            </div>
          }
          confirmDisabled={Boolean(
            actionApprovalRequests[0].requiresRiskAcknowledgement &&
            riskAcknowledgedRequestId !== actionApprovalRequests[0].requestId
          )}
          initialFocus="none"
          isSubmitting={resolvingActionApprovalId === actionApprovalRequests[0].requestId}
          onClose={() => void resolveActionApproval(false)}
          onConfirm={() => void resolveActionApproval(true)}
          title={actionApprovalRequests[0].title}
        />
      ) : null}
      {isMainWorkspaceWindow && sessionSecurity.isLocked ? (
        sessionSecurity.status === 'error' ? (
          <SessionLockScreen mode="error" onRetry={sessionSecurity.retry} />
        ) : (
          <SessionLockScreen mode="locked" onUnlock={sessionSecurity.unlock} />
        )
      ) : null}
    </>
  )
}

function ConnectionImportPlanPortal({
  plan,
  onClose,
  onCommit
}: {
  plan: NonNullable<AppViewModel['shell']['connectionImportPlan']>
  onClose(): void
  onCommit: AppViewModel['shell']['commitConnectionJsonPreview']
}) {
  return <ConnectionImportPreviewModal plan={plan} onClose={onClose} onCommit={onCommit} />
}
