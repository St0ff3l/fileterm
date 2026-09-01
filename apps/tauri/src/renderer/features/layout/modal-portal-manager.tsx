import { lazy, Suspense, type ComponentProps, type ElementType, type ReactNode } from 'react'
import { CommandManagerModal } from '../commands/command-manager-modal'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { ConnectionFormHost } from '../connections/connection-form-host'
import { ConnectionManagerModal } from '../connections/connection-manager-modal'
import { SshCredentialsModal } from '../connections/ssh-credentials-modal'
import { SshHostVerificationModal } from '../connections/ssh-host-verification-modal'
import { SshKeyboardInteractiveModal } from '../connections/ssh-keyboard-interactive-modal'
import { SshKeyPassphraseModal } from '../connections/ssh-key-passphrase-modal'
import { BackupPasswordModal } from '../connections/backup-password-modal'
import { SudoPasswordPromptModal } from '../connections/sudo-password-prompt-modal'
import { FileActionModal } from '../files/file-action-modal'
import { FilePermissionModal } from '../files/file-permission-modal'
import { RootAccessModal } from '../files/root-access-modal'
import { SmbCredentialsModal } from '../files/smb-credentials-modal'
import { SmbSharePickerModal } from '../files/smb-share-picker-modal'
import { SettingsModal } from '../settings/settings-modal'
import { TabContextMenu } from './tab-context-menu'

const FileEditorModal = lazy(() =>
  import('../files/file-editor-modal').then((module) => ({
    default: module.FileEditorModal
  }))
)

export type ModalBinding<T extends ElementType> = ComponentProps<T> | null
export type NonStandaloneModalBinding<T extends ElementType> = Omit<ComponentProps<T>, 'standalone'> | null

export type ConnectionManagerModalBinding = NonStandaloneModalBinding<typeof ConnectionManagerModal>
export type CommandManagerModalBinding = NonStandaloneModalBinding<typeof CommandManagerModal>
export type SettingsModalBinding = ModalBinding<typeof SettingsModal>
export type ConnectionFormModalBinding = NonStandaloneModalBinding<typeof ConnectionFormHost>
export type FilePermissionModalBinding = ModalBinding<typeof FilePermissionModal>
export type RootAccessModalBinding = ModalBinding<typeof RootAccessModal>
export type SmbCredentialsModalBinding = ModalBinding<typeof SmbCredentialsModal>
export type SmbSharePickerModalBinding = ModalBinding<typeof SmbSharePickerModal>
export type SshCredentialsModalBinding = ModalBinding<typeof SshCredentialsModal>
export type SshHostVerificationModalBinding = ModalBinding<typeof SshHostVerificationModal>
export type SshKeyboardInteractiveModalBinding = ModalBinding<typeof SshKeyboardInteractiveModal>
export type SshKeyPassphraseModalBinding = ModalBinding<typeof SshKeyPassphraseModal>
export type BackupPasswordModalBinding = ModalBinding<typeof BackupPasswordModal>
export type SudoPasswordPromptModalBinding = ModalBinding<typeof SudoPasswordPromptModal>
export type ConfirmActionDialogBinding = ModalBinding<typeof ConfirmActionDialog>
export type FileEditorModalBinding = NonStandaloneModalBinding<typeof FileEditorModal>
export type TabContextMenuBinding = ModalBinding<typeof TabContextMenu>

export interface SshInteractionPortalProps {
  sshCredentials: SshCredentialsModalBinding
  sshHostVerification: SshHostVerificationModalBinding
  sshKeyboardInteractive: SshKeyboardInteractiveModalBinding
  sshKeyPassphrase: SshKeyPassphraseModalBinding
}

export type FileActionModalBinding =
  | {
      kind: 'delete'
      props: ComponentProps<typeof ConfirmActionDialog>
    }
  | {
      kind: 'action'
      props: ComponentProps<typeof FileActionModal>
    }
  | null

export interface ModalPortalManagerProps extends SshInteractionPortalProps {
  commandManager: CommandManagerModalBinding
  connectionForm: ConnectionFormModalBinding
  connectionManager: ConnectionManagerModalBinding
  fileAction: FileActionModalBinding
  fileEditor?: FileEditorModalBinding
  fileEditorFallback?: ReactNode
  filePermission: FilePermissionModalBinding
  rootAccess: RootAccessModalBinding
  smbCredentials: SmbCredentialsModalBinding
  smbSharePicker: SmbSharePickerModalBinding
  settings: SettingsModalBinding
  shortcutCloseConfirm: ConfirmActionDialogBinding
  backupPassword: BackupPasswordModalBinding
  sudoPasswordPrompt: SudoPasswordPromptModalBinding
  tabContextMenu?: TabContextMenuBinding
  windowCloseConfirm: ConfirmActionDialogBinding
}

export function SshInteractionPortal({
  sshCredentials,
  sshHostVerification,
  sshKeyboardInteractive,
  sshKeyPassphrase
}: SshInteractionPortalProps) {
  return (
    <>
      {sshCredentials ? <SshCredentialsModal {...sshCredentials} /> : null}
      {sshHostVerification ? <SshHostVerificationModal {...sshHostVerification} /> : null}
      {sshKeyboardInteractive ? <SshKeyboardInteractiveModal {...sshKeyboardInteractive} /> : null}
      {sshKeyPassphrase ? <SshKeyPassphraseModal {...sshKeyPassphrase} /> : null}
    </>
  )
}

export function ModalPortalManager({
  commandManager,
  connectionForm,
  connectionManager,
  fileAction,
  fileEditor,
  fileEditorFallback = null,
  filePermission,
  rootAccess,
  smbCredentials,
  smbSharePicker,
  settings,
  shortcutCloseConfirm,
  sshCredentials,
  sshHostVerification,
  sshKeyboardInteractive,
  sshKeyPassphrase,
  backupPassword,
  sudoPasswordPrompt,
  tabContextMenu,
  windowCloseConfirm
}: ModalPortalManagerProps) {
  return (
    <>
      {tabContextMenu ? <TabContextMenu {...tabContextMenu} /> : null}
      {connectionManager ? <ConnectionManagerModal {...connectionManager} standalone={false} /> : null}
      {commandManager ? <CommandManagerModal {...commandManager} standalone={false} /> : null}
      {settings ? <SettingsModal {...settings} /> : null}
      {connectionForm ? <ConnectionFormHost {...connectionForm} standalone={false} /> : null}
      {fileEditor ? (
        <Suspense fallback={fileEditorFallback}>
          <FileEditorModal {...fileEditor} standalone={false} />
        </Suspense>
      ) : null}
      {fileAction?.kind === 'delete' ? (
        <ConfirmActionDialog {...fileAction.props} />
      ) : fileAction?.kind === 'action' ? (
        <FileActionModal {...fileAction.props} />
      ) : null}
      {filePermission ? <FilePermissionModal {...filePermission} /> : null}
      {rootAccess ? <RootAccessModal {...rootAccess} /> : null}
      {smbCredentials ? <SmbCredentialsModal {...smbCredentials} /> : null}
      {smbSharePicker ? <SmbSharePickerModal {...smbSharePicker} /> : null}
      <SshInteractionPortal
        sshCredentials={sshCredentials}
        sshHostVerification={sshHostVerification}
        sshKeyboardInteractive={sshKeyboardInteractive}
        sshKeyPassphrase={sshKeyPassphrase}
      />
      {backupPassword ? <BackupPasswordModal {...backupPassword} /> : null}
      {sudoPasswordPrompt ? <SudoPasswordPromptModal {...sudoPasswordPrompt} /> : null}
      {shortcutCloseConfirm ? <ConfirmActionDialog {...shortcutCloseConfirm} /> : null}
      {windowCloseConfirm ? <ConfirmActionDialog {...windowCloseConfirm} /> : null}
    </>
  )
}
