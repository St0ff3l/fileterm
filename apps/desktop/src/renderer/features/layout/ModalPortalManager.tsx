import { lazy, Suspense, type ComponentProps, type ElementType, type ReactNode } from 'react'
import { CommandManagerModal } from '../commands/CommandManagerModal'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { ConnectionFormHost } from '../connections/ConnectionFormHost'
import { ConnectionManagerModal } from '../connections/ConnectionManagerModal'
import { SshCredentialsModal } from '../connections/SshCredentialsModal'
import { SshHostVerificationModal } from '../connections/SshHostVerificationModal'
import { SshKeyPassphraseModal } from '../connections/SshKeyPassphraseModal'
import { SshKeyboardInteractiveModal } from '../connections/SshKeyboardInteractiveModal'
import { FileActionModal } from '../files/FileActionModal'
import { FilePermissionModal } from '../files/FilePermissionModal'
import { RootAccessModal } from '../files/RootAccessModal'
import { SettingsModal } from '../settings/SettingsModal'
import { TabContextMenu } from './TabContextMenu'

const FileEditorModal = lazy(() =>
  import('../files/FileEditorModal').then((module) => ({
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
export type SshCredentialsModalBinding = ModalBinding<typeof SshCredentialsModal>
export type SshHostVerificationModalBinding = ModalBinding<typeof SshHostVerificationModal>
export type SshKeyPassphraseModalBinding = ModalBinding<typeof SshKeyPassphraseModal>
export type SshKeyboardInteractiveModalBinding = ModalBinding<typeof SshKeyboardInteractiveModal>
export type ConfirmActionDialogBinding = ModalBinding<typeof ConfirmActionDialog>
export type FileEditorModalBinding = NonStandaloneModalBinding<typeof FileEditorModal>
export type TabContextMenuBinding = ModalBinding<typeof TabContextMenu>

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

export interface ModalPortalManagerProps {
  commandManager: CommandManagerModalBinding
  connectionForm: ConnectionFormModalBinding
  connectionManager: ConnectionManagerModalBinding
  fileAction: FileActionModalBinding
  fileEditor?: FileEditorModalBinding
  fileEditorFallback?: ReactNode
  filePermission: FilePermissionModalBinding
  rootAccess: RootAccessModalBinding
  settings: SettingsModalBinding
  shortcutCloseConfirm: ConfirmActionDialogBinding
  sshCredentials: SshCredentialsModalBinding
  sshHostVerification: SshHostVerificationModalBinding
  sshKeyPassphrase: SshKeyPassphraseModalBinding
  sshKeyboardInteractive: SshKeyboardInteractiveModalBinding
  tabContextMenu?: TabContextMenuBinding
  windowCloseConfirm: ConfirmActionDialogBinding
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
  settings,
  shortcutCloseConfirm,
  sshCredentials,
  sshHostVerification,
  sshKeyPassphrase,
  sshKeyboardInteractive,
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
      {sshCredentials ? <SshCredentialsModal {...sshCredentials} /> : null}
      {sshHostVerification ? <SshHostVerificationModal {...sshHostVerification} /> : null}
      {sshKeyPassphrase ? <SshKeyPassphraseModal {...sshKeyPassphrase} /> : null}
      {sshKeyboardInteractive ? <SshKeyboardInteractiveModal {...sshKeyboardInteractive} /> : null}
      {shortcutCloseConfirm ? <ConfirmActionDialog {...shortcutCloseConfirm} /> : null}
      {windowCloseConfirm ? <ConfirmActionDialog {...windowCloseConfirm} /> : null}
    </>
  )
}
