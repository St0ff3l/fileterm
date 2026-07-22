import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { t } from '../../i18n'

export function RootAccessModal({
  defaultSshUser,
  defaultSudoUser,
  errorMessage,
  isSubmitting = false,
  onClose,
  onSubmit
}: {
  defaultSshUser?: string
  defaultSudoUser?: string
  errorMessage?: string | null
  isSubmitting?: boolean
  onClose(): void
  onSubmit(input: { sudoUser: string; sudoPassword: string }): void
}) {
  const [sudoUser, setSudoUser] = useState(defaultSudoUser || 'root')
  const [sudoPassword, setSudoPassword] = useState('')

  useEffect(() => {
    setSudoUser(defaultSudoUser || 'root')
    setSudoPassword('')
  }, [defaultSudoUser])

  return (
    <div className="modal-backdrop">
      <div className="modal-card root-access-modal">
        <div className="root-access-modal__header">
          <span className="root-access-modal__title">
            <span aria-hidden="true" className="material-symbols-outlined">
              admin_panel_settings
            </span>
            <span>{t.fileRootAccessTitle}</span>
          </span>
          <CloseButton className="root-access-modal__close" disabled={isSubmitting} onClick={onClose} />
        </div>

        <div className="root-access-modal__body">
          <fieldset className="ssh-fieldset root-access-fieldset">
            <legend>{t.general}</legend>
            <div className="root-access-description">{t.fileRootAccessDescription}</div>

            <div className="root-access-meta">
              <span>{t.fileRootAccessLoginUser}</span>
              <strong>{defaultSshUser || '-'}</strong>
            </div>
          </fieldset>

          <fieldset className="ssh-fieldset root-access-fieldset" disabled={isSubmitting}>
            <legend>{t.auth}</legend>
            <label className="file-action-field">
              <span>{t.fileRootAccessTargetUser}</span>
              <input
                value={sudoUser}
                onChange={(event) => setSudoUser(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    onSubmit({ sudoUser, sudoPassword })
                  }
                }}
              />
            </label>

            <label className="file-action-field">
              <span>{t.fileRootAccessPassword}</span>
              <input
                autoFocus
                type="password"
                value={sudoPassword}
                onChange={(event) => setSudoPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    onSubmit({ sudoUser, sudoPassword })
                  }
                }}
              />
            </label>
          </fieldset>

          <div className="root-access-note" role="note">
            <div className="root-access-note-title">{t.fileRootAccessPasswordHint}</div>
            <div className="root-access-note-body">{t.fileRootAccessPasswordHintDetail}</div>
          </div>
          {errorMessage ? (
            <div className="modal-error" role="alert">
              {errorMessage}
            </div>
          ) : null}

          <div className="form-actions">
            <button className="flat-button" disabled={isSubmitting} onClick={onClose} type="button">
              {t.cancel}
            </button>
            <button
              className="primary-button file-action-submit-button"
              disabled={isSubmitting}
              onClick={() => onSubmit({ sudoUser, sudoPassword })}
              type="button"
            >
              {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
              <span>{t.fileRootAccessConfirm}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
