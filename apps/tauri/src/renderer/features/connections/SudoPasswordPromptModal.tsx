import { useEffect, useState } from 'react'
import type { SudoPasswordRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { StableButtonContent } from '../common/StableButtonContent'

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
  useEffect(() => {
    setValue('')
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
      <div className="modal-card root-access-modal sudo-password-prompt-modal">
        <div className="root-access-modal__header">
          <span className="root-access-modal__title">
            <AppIcon name="shield-check" size={16} />
            <span>{request.kind === 'sudo' ? t.sudoPasswordTitle : t.suPasswordTitle}</span>
          </span>
          <CloseButton className="root-access-modal__close" disabled={isSubmitting} onClick={onCancel} />
        </div>
        <div className="root-access-modal__body">
          <fieldset className="ssh-fieldset root-access-fieldset">
            <legend>{t.general}</legend>
            <div className="root-access-description">{t.sudoPasswordDescription}</div>
            <div className="root-access-meta">
              <span>{t.sudoPasswordTarget}</span>
              <strong>{target}</strong>
              {request.cwd ? <span className="sudo-password-cwd">{request.cwd}</span> : null}
            </div>
          </fieldset>

          <fieldset className="ssh-fieldset root-access-fieldset" disabled={isSubmitting}>
            <legend>{t.auth}</legend>
            <div className="file-action-field">
              <span>{t.sudoPasswordCommand}</span>
              <div
                aria-label={t.sudoPasswordCommand}
                aria-readonly="true"
                className="sudo-password-command"
                role="textbox"
              >
                {request.command}
              </div>
            </div>
            <label className="file-action-field">
              <span>{t.sudoPasswordLabel}</span>
              <input
                autoFocus
                disabled={isSubmitting}
                type="password"
                value={value}
                placeholder={t.sudoPasswordPlaceholder}
                onChange={(event) => setValue(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') submit(false)
                }}
              />
            </label>
          </fieldset>

          <div className="root-access-note" role="note">
            <div className="root-access-note-title">{t.fileRootAccessPasswordHint}</div>
            <div className="root-access-note-body">{t.sudoPasswordPrivacy}</div>
          </div>
          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        </div>
        <div className="form-actions root-access-modal__actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="flat-button" disabled={!value || isSubmitting} onClick={() => submit(false)} type="button">
            {t.sudoPasswordOneTime}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button"
            disabled={!value || isSubmitting}
            onClick={() => submit(true)}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.sudoPasswordSaveAndExecute} />
          </button>
        </div>
      </div>
    </div>
  )
}
