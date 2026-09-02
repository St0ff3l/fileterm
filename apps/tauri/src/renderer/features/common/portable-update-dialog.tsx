import { t } from '../../i18n'
import { ConfirmActionDialog } from './confirm-action-dialog'

export function PortableUpdateDialog({ onClose, onOpenReleasePage }: { onClose(): void; onOpenReleasePage(): void }) {
  return (
    <ConfirmActionDialog
      confirmLabel={t.openReleasePage}
      confirmVariant="primary"
      description={t.portableUpdateDescription}
      onClose={onClose}
      onConfirm={() => {
        onOpenReleasePage()
        onClose()
      }}
      title={t.portableUpdateTitle}
    />
  )
}
