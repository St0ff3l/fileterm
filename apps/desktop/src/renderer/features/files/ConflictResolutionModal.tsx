import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

export function ConflictResolutionModal({
  name,
  onClose,
  onResolve
}: {
  name: string
  onClose(): void
  onResolve(choice: 'skip' | 'keep-both' | 'replace'): void
}) {
  return (
    <div className="modal-backdrop">
      <div className="modal-card conflict-resolution-modal">
        <div className="modal-header">
          <span>{t.conflictDialogTitle}</span>
          <button className="icon-button" onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>
        <div className="file-action-description">{t.conflictDialogDescription}</div>
        <div className="conflict-resolution-name">{name}</div>
        <div className="conflict-resolution-actions">
          <button className="flat-button" onClick={() => onResolve('skip')} type="button">{t.conflictSkip}</button>
          <button className="flat-button" onClick={() => onResolve('keep-both')} type="button">{t.conflictKeepBoth}</button>
          <button className="primary-button" onClick={() => onResolve('replace')} type="button">{t.conflictReplace}</button>
        </div>
      </div>
    </div>
  )
}
