import { useEffect, useState } from 'react'
import type { SudoPasswordRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

/** Local-only one-time sudo/su password prompt for an isolated exec channel. */
export function SudoPasswordPromptModal({
  request,
  errorMessage,
  isSubmitting = false,
  onCancel,
  onSubmit
}: {
  request: SudoPasswordRequest
  errorMessage?: string | null
  isSubmitting?: boolean
  onCancel(): void
  onSubmit(value: string, save: boolean): void
}) {
  const [value, setValue] = useState('')
  const [showValue, setShowValue] = useState(false)
  useEffect(() => {
    setValue('')
    setShowValue(false)
  }, [request.requestId])

  const target = [request.shellUser, request.host].filter(Boolean).join('@') || request.host
  const submit = (save: boolean) => {
    if (!value || isSubmitting) return
    const password = value
    setValue('')
    onSubmit(password, save)
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal sudo-password-prompt-modal">
        <div className="modal-header">
          <span>{request.kind === 'sudo' ? t.sudoPasswordTitle : t.suPasswordTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>
        <p className="root-access-description">{t.sudoPasswordDescription}</p>
        <div className="root-access-meta">
          <span>{t.sudoPasswordTarget}</span>
          <strong>{target}</strong>
          {request.cwd ? <span>{request.cwd}</span> : null}
        </div>
        <label className="file-action-field">
          <span>{t.sudoPasswordCommand}</span>
          <input disabled readOnly value={request.command} />
        </label>
        <label className="file-action-field">
          <span>{t.sudoPasswordLabel}</span>
          <div className="sudo-password-input-row">
            <input
              autoFocus
              disabled={isSubmitting}
              type={showValue ? 'text' : 'password'}
              value={value}
              placeholder={t.sudoPasswordPlaceholder}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') submit(false)
              }}
            />
            <button
              aria-label={showValue ? t.hidePassword : t.showPassword}
              className="flat-button compact sudo-password-visibility"
              disabled={isSubmitting}
              onClick={() => setShowValue((current) => !current)}
              type="button"
            >
              {showValue ? t.hidePassword : t.showPassword}
            </button>
          </div>
        </label>
        <p className="file-action-hint">{t.sudoPasswordPrivacy}</p>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="flat-button" disabled={!value || isSubmitting} onClick={() => submit(false)} type="button">
            {t.sudoPasswordOneTime}
          </button>
          <button
            className="primary-button"
            disabled={!value || isSubmitting}
            onClick={() => submit(true)}
            type="button"
          >
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.sudoPasswordSaveAndExecute}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
