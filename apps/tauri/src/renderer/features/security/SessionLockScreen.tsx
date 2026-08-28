import { useEffect, useRef, useState, type FormEvent } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'

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
  const [isShaking, setIsShaking] = useState(false)

  useEffect(() => {
    if (mode === 'locked') {
      passwordInputRef.current?.focus()
    }
  }, [mode])

  const triggerShake = () => {
    setIsShaking(true)
    setTimeout(() => setIsShaking(false), 450)
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!onUnlock || isSubmitting) return
    if (!password) {
      setErrorMessage(t.securityUnlockRequired)
      triggerShake()
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
      triggerShake()
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
      <div className={['session-lock-card', isShaking && 'session-lock-card--shake'].filter(Boolean).join(' ')}>
        <div className="session-lock-card__mark" aria-hidden="true">
          <AppIcon name={mode === 'error' ? 'shield' : 'shield-check'} size={28} strokeWidth={1.5} />
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
          <button className="session-lock-card__retry-button" onClick={onRetry} type="button">
            <AppIcon name="refresh" size={14} />
            <span>{t.securityRetry}</span>
          </button>
        ) : mode === 'locked' ? (
          <form className="session-lock-card__form" onSubmit={(event) => void submit(event)}>
            <div className="session-lock-card__input-wrapper">
              <AppIcon className="session-lock-card__input-icon" name="lock" size={14} strokeWidth={1.35} />
              <input
                ref={passwordInputRef}
                aria-label={t.securityUnlockPassword}
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
              <button
                aria-busy={isSubmitting}
                aria-label={t.securityUnlock}
                className={['session-lock-card__submit-button', password && 'is-active'].filter(Boolean).join(' ')}
                disabled={!password || isSubmitting}
                title={t.securityUnlock}
                type="submit"
              >
                {isSubmitting ? (
                  <span aria-hidden="true" className="session-lock-card__submit-spinner button-spinner" />
                ) : (
                  <AppIcon name="arrow-right" size={14} strokeWidth={2.2} />
                )}
              </button>
            </div>
            {errorMessage ? (
              <p className="session-lock-card__error" role="alert">
                {errorMessage}
              </p>
            ) : null}
          </form>
        ) : (
          <span aria-hidden="true" className="session-lock-card__spinner button-spinner" />
        )}
      </div>
    </div>
  )
}
