import { useEffect, useRef, useState, type FormEvent } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { StableButtonContent } from '../common/StableButtonContent'

export function SessionLockScreen({
  mode,
  onRetry,
  onUnlock
}: {
  mode: 'loading' | 'locked' | 'error'
  onRetry?: () => void
  onUnlock?: (password: string) => Promise<boolean>
}) {
  const passwordInputRef = useRef<HTMLInputElement>(null)
  const [password, setPassword] = useState('')
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)

  useEffect(() => {
    if (mode === 'locked') {
      passwordInputRef.current?.focus()
    }
  }, [mode])

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!onUnlock || isSubmitting) return
    if (!password) {
      setErrorMessage(t.securityUnlockRequired)
      passwordInputRef.current?.focus()
      return
    }

    setIsSubmitting(true)
    setErrorMessage(null)
    const valid = await onUnlock(password)
    setPassword('')
    setIsSubmitting(false)
    if (!valid) {
      setErrorMessage(t.securityUnlockFailed)
      passwordInputRef.current?.focus()
    }
  }

  return (
    <div
      aria-busy={mode === 'loading' || isSubmitting}
      aria-live="polite"
      className={`session-lock-screen session-lock-screen--${mode}`}
      onKeyDown={(event) => event.stopPropagation()}
      onPointerDown={(event) => event.stopPropagation()}
      role="dialog"
      aria-modal="true"
    >
      <div className="session-lock-screen__grid" aria-hidden="true" />
      <div className="session-lock-card">
        <div className="session-lock-card__eyebrow">
          <AppIcon name={mode === 'error' ? 'shield' : 'shield-check'} size={16} />
          <span>FILETERM / SECURE SESSION</span>
        </div>
        <div className="session-lock-card__mark" aria-hidden="true">
          <AppIcon name={mode === 'error' ? 'shield' : 'shield-check'} size={34} strokeWidth={1.45} />
        </div>
        <h1>
          {mode === 'loading' ? t.securityLoading : mode === 'error' ? t.securityLoadFailed : t.securityLockedTitle}
        </h1>
        <p className="session-lock-card__description">
          {mode === 'loading'
            ? t.securityLoadingHint
            : mode === 'error'
              ? t.securityLoadFailedHint
              : t.securityLockedDescription}
        </p>

        {mode === 'error' ? (
          <button className="primary-button session-lock-card__action" onClick={onRetry} type="button">
            <AppIcon name="refresh" size={14} />
            <span>{t.securityRetry}</span>
          </button>
        ) : mode === 'locked' ? (
          <form className="session-lock-card__form" onSubmit={(event) => void submit(event)}>
            <label className="session-lock-card__input-label" htmlFor="session-lock-password">
              {t.securityUnlockPassword}
            </label>
            <input
              ref={passwordInputRef}
              autoComplete="current-password"
              disabled={isSubmitting}
              id="session-lock-password"
              onChange={(event) => {
                setPassword(event.target.value)
                setErrorMessage(null)
              }}
              placeholder={t.securityUnlockPasswordPlaceholder}
              type="password"
              value={password}
            />
            {errorMessage ? (
              <p className="session-lock-card__error" role="alert">
                {errorMessage}
              </p>
            ) : null}
            <button
              aria-busy={isSubmitting}
              className="primary-button session-lock-card__action"
              disabled={isSubmitting}
              type="submit"
            >
              <StableButtonContent
                busy={isSubmitting}
                icon={<AppIcon name="check" size={14} />}
                label={t.securityUnlock}
              />
            </button>
          </form>
        ) : (
          <span aria-hidden="true" className="session-lock-card__spinner button-spinner" />
        )}
      </div>
    </div>
  )
}
