import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'
import { StableButtonContent } from '../common/StableButtonContent'
import { t } from '../../i18n'

const SAVED_PASSWORD_MASK = '••••••••••••'

export function RootAccessModal({
  defaultSshUser,
  defaultRootAccessMethod,
  defaultSudoUser,
  errorMessage,
  hasSavedSudoPassword = false,
  hasSavedSuPassword = false,
  isSubmitting = false,
  onClose,
  onSubmit
}: {
  defaultSshUser?: string
  defaultRootAccessMethod?: 'sudo' | 'su'
  defaultSudoUser?: string
  errorMessage?: string | null
  hasSavedSudoPassword?: boolean
  hasSavedSuPassword?: boolean
  isSubmitting?: boolean
  onClose(): void
  onSubmit(input: { rootAccessMethod: 'sudo' | 'su'; sudoUser: string; sudoPassword: string }): void
}) {
  const [rootAccessMethod, setRootAccessMethod] = useState<'sudo' | 'su'>(defaultRootAccessMethod || 'sudo')
  const [sudoUser, setSudoUser] = useState(defaultSudoUser || 'root')
  const [sudoPassword, setSudoPassword] = useState('')
  const [usesSavedPassword, setUsesSavedPassword] = useState(false)

  const hasSavedPassword = rootAccessMethod === 'sudo' ? hasSavedSudoPassword : hasSavedSuPassword

  useEffect(() => {
    setRootAccessMethod(defaultRootAccessMethod || 'sudo')
    setSudoUser(defaultSudoUser || 'root')
    setSudoPassword('')
  }, [defaultRootAccessMethod, defaultSudoUser])

  useEffect(() => {
    setUsesSavedPassword(hasSavedPassword)
    setSudoPassword(hasSavedPassword ? SAVED_PASSWORD_MASK : '')
  }, [hasSavedPassword])

  const submit = () =>
    onSubmit({
      rootAccessMethod,
      sudoUser,
      // The mask is never a credential. An untouched masked field means the
      // backend should select the saved password for the chosen method.
      sudoPassword: usesSavedPassword ? '' : sudoPassword
    })

  const startPasswordReplacement = () => {
    if (!usesSavedPassword) return
    setUsesSavedPassword(false)
    setSudoPassword('')
  }

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
              <span>{t.fileRootAccessMethod}</span>
              <DropdownSelect
                className="root-access-method-select"
                disabled={isSubmitting}
                value={rootAccessMethod}
                options={[
                  { value: 'sudo', label: t.fileRootAccessMethodSudo },
                  { value: 'su', label: t.fileRootAccessMethodSu }
                ]}
                onChange={(value) => setRootAccessMethod(value as 'sudo' | 'su')}
              />
              <div className="file-action-hint root-access-method-hint">{t.fileRootAccessMethodHint}</div>
            </label>

            <label className="file-action-field">
              <span>{t.fileRootAccessTargetUser}</span>
              <input
                value={sudoUser}
                onChange={(event) => setSudoUser(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    submit()
                  }
                }}
              />
            </label>

            <label className="file-action-field">
              <span>{t.fileRootAccessPassword}</span>
              <input
                autoFocus={!hasSavedPassword}
                readOnly={usesSavedPassword}
                type="password"
                value={sudoPassword}
                onFocus={startPasswordReplacement}
                onChange={(event) => setSudoPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    submit()
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
        </div>

        <div className="form-actions root-access-modal__actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onClose} type="button">
            {t.cancel}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button file-action-submit-button"
            disabled={isSubmitting}
            onClick={submit}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.fileRootAccessConfirm} />
          </button>
        </div>
      </div>
    </div>
  )
}
