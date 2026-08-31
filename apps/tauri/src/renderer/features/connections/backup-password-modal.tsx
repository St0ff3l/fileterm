import { useEffect, useState } from 'react'
import type { BackupPasswordRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/close-button'
import { StableButtonContent } from '../common/stable-button-content'

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
  const [confirmation, setConfirmation] = useState('')
  const requiresConfirmation = request.operation === 'upload'
  useEffect(() => {
    setValue('')
    setConfirmation('')
  }, [request.requestId])

  const hasUppercase = /[A-Z]/.test(value)
  const hasLowercase = /[a-z]/.test(value)
  const hasValidLength = value.length >= 8
  const isMatching = value === confirmation
  const isValid = hasValidLength && hasUppercase && hasLowercase && (!requiresConfirmation || isMatching)

  const submit = () => {
    if (!isValid || isSubmitting) return
    const password = value
    setValue('')
    setConfirmation('')
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
        {requiresConfirmation ? (
          <label className="file-action-field">
            <span>{t.backupPasswordConfirmLabel}</span>
            <input
              disabled={isSubmitting}
              type="password"
              value={confirmation}
              placeholder={t.backupPasswordConfirmPlaceholder}
              onChange={(event) => setConfirmation(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') submit()
              }}
            />
          </label>
        ) : null}
        <p className="file-action-hint">{t.backupPasswordPolicy}</p>
        {requiresConfirmation && confirmation && !isMatching ? (
          <div className="modal-error">{t.backupPasswordMismatch}</div>
        ) : null}
        <p className="file-action-hint">{t.backupPasswordPrivacy}</p>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button"
            disabled={!isValid || isSubmitting}
            onClick={submit}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.backupPasswordContinue} />
          </button>
        </div>
      </div>
    </div>
  )
}
