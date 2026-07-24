import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'
import { t } from '../../i18n'

export function SmbSharePickerModal({
  errorMessage,
  isSubmitting = false,
  path,
  shares,
  onCancel,
  onChangeCredentials,
  onSubmit
}: {
  errorMessage?: string | null
  isSubmitting?: boolean
  path: string
  shares: string[]
  onCancel(): void
  onChangeCredentials(): void
  onSubmit(share: string): void
}) {
  const [selectedShare, setSelectedShare] = useState(shares[0] ?? '')

  useEffect(() => {
    setSelectedShare(shares[0] ?? '')
  }, [path, shares])

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal network-share-picker-modal">
        <div className="network-share-modal__header">
          <span className="network-share-modal__title">
            <span aria-hidden="true" className="material-symbols-outlined">
              folder_shared
            </span>
            <span>{t.networkShareSelectTitle}</span>
          </span>
          <CloseButton className="network-share-modal__close" disabled={isSubmitting} onClick={onCancel} />
        </div>

        <div className="network-share-modal__body">
          <fieldset className="ssh-fieldset network-share-fieldset" disabled={isSubmitting}>
            <legend>{t.general}</legend>
            <div className="network-share-description">{t.networkShareSelectDescription}</div>
            <div className="network-share-path-value">
              <span>{t.networkShareCredentialsPath}</span>
              <strong title={path}>{path}</strong>
            </div>

            <label className="file-action-field">
              <span>{t.networkShareSelectFolder}</span>
              <DropdownSelect
                autoFocus
                className="network-share-select-shell"
                disabled={isSubmitting}
                value={selectedShare}
                options={shares.map((share) => ({ value: share, label: share }))}
                onChange={(value) => setSelectedShare(value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    onSubmit(selectedShare)
                  }
                }}
              />
            </label>
          </fieldset>

          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

          <div className="form-actions">
            <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
              {t.cancel}
            </button>
            <button className="flat-button" disabled={isSubmitting} onClick={onChangeCredentials} type="button">
              {t.networkShareSelectChangeCredentials}
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
    </div>
  )
}
