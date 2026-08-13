import { useEffect, useState } from 'react'
import type { BackupPasswordRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

/** One-time password dialog for encrypted WebDAV/S3 backup sync. */
export function BackupPasswordModal({
  request,
  errorMessage,
  isSubmitting = false,
  onCancel,
  onSubmit
}: {
  request: BackupPasswordRequest
  errorMessage?: string | null
  isSubmitting?: boolean
  onCancel(): void
  onSubmit(value: string): void
}) {
  const [value, setValue] = useState('')
  useEffect(() => setValue(''), [request.requestId])

  const submit = () => {
    if (!value || isSubmitting) return
    const password = value
    setValue('')
    onSubmit(password)
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal backup-password-modal">
        <div className="modal-header">
          <span>{t.backupPasswordTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>
        <p className="root-access-description">
          {request.operation === 'upload' ? t.backupPasswordUploadDescription : t.backupPasswordDownloadDescription}
        </p>
        <div className="root-access-meta">
          <span>{t.backupPasswordProvider}</span>
          <strong>{request.provider}</strong>
        </div>
        <label className="file-action-field">
          <span>{t.backupPasswordLabel}</span>
          <input
            autoFocus
            disabled={isSubmitting}
            type="password"
            value={value}
            placeholder={t.backupPasswordPlaceholder}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') submit()
            }}
          />
        </label>
        <p className="file-action-hint">{t.backupPasswordPrivacy}</p>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="primary-button" disabled={!value || isSubmitting} onClick={submit} type="button">
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.backupPasswordContinue}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
