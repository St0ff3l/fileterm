import { t } from '../../i18n'

export function TransferBar({
  activeCount,
  fullWidth = false,
  isPending,
  onOpen
}: {
  activeCount: number
  fullWidth?: boolean
  isPending: boolean
  onOpen(): void
}) {
  return (
    <footer className={`transfer-strip ${fullWidth ? 'full-width' : ''}`}>
      <strong>{t.transferTasks}</strong>
      <button className="transfer-summary-button" onClick={onOpen} type="button">
        {activeCount > 0 ? `${activeCount} ${t.runningTasks}` : isPending ? t.updating : `0 ${t.runningTasks}`}
      </button>
    </footer>
  )
}
