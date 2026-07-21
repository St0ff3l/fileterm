import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { t } from '../../i18n'

export function SmbSharePickerModal({
  errorMessage,
  isSubmitting = false,
  path,
  shares,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  isSubmitting?: boolean
  path: string
  shares: string[]
  onCancel(): void
  onSubmit(share: string): void
}) {
  const [selectedShare, setSelectedShare] = useState(shares[0] ?? '')

  useEffect(() => {
    setSelectedShare(shares[0] ?? '')
  }, [path, shares])

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal network-share-picker-modal">
        <div className="modal-header">
          <span>{t.networkShareSelectTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>

        <div className="root-access-description">{t.networkShareSelectDescription}</div>

        <div className="root-access-meta network-share-credentials-path">
          <span>{t.networkShareCredentialsPath}</span>
          <strong title={path}>{path}</strong>
        </div>

        <label className="file-action-field">
          <span>{t.networkShareSelectFolder}</span>
          <span className="ft-select-shell network-share-select-shell">
            <select
              autoFocus
              disabled={isSubmitting}
              value={selectedShare}
              onChange={(event) => setSelectedShare(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !isSubmitting) {
                  onSubmit(selectedShare)
                }
              }}
            >
              {shares.map((share) => (
                <option key={share} value={share}>
                  {share}
                </option>
              ))}
            </select>
            <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
              expand_more
            </span>
          </span>
        </label>

        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button
            className="primary-button"
            disabled={isSubmitting || !selectedShare}
            onClick={() => onSubmit(selectedShare)}
            type="button"
          >
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.networkShareSelectConfirm}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
