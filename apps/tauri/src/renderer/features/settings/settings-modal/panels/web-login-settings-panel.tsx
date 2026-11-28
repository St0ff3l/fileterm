import { useEffect, useRef, useState, type FormEvent } from 'react'
import { saveWebLogin, type WebLoginStatus } from '../../../../../bridge/web-login'
import { Button } from '../../../../components/common/button/button'
import { Input } from '../../../../components/common/input/input'
import { AppIcon } from '../../../common/app-icon'
import { t } from '../../../../i18n'
import appDefaultIcon from '../../../../../../src-tauri/icons/app/shared/classic-512.png'
import './web-login-settings-panel.css'

const MIN_WEB_LOGIN_PASSWORD_LENGTH = 8
const USERNAME_PATTERN = /^[A-Za-z0-9._-]{1,64}$/

type ErrorField = 'username' | 'currentPassword' | 'password' | null

function PasswordToggleButton({
  visible,
  disabled,
  onToggle
}: {
  visible: boolean
  disabled: boolean
  onToggle(): void
}) {
  const label = visible ? t.hidePassword : t.showPassword
  return (
    <button
      aria-label={label}
      className="web-login-password-toggle"
      disabled={disabled}
      onClick={onToggle}
      title={label}
      type="button"
    >
      <AppIcon name={visible ? 'eye-off' : 'eye'} size={15} />
    </button>
  )
}

export function WebLoginSettingsPanel({
  status,
  onSaved,
  standalone = false
}: {
  status: WebLoginStatus
  onSaved(): void
  standalone?: boolean
}) {
  const { mode } = status
  const initialUsername = status.mode === 'managed' ? status.username : ''
  const [username, setUsername] = useState(initialUsername)
  const [password, setPassword] = useState('')
  const [currentPassword, setCurrentPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [showCurrentPassword, setShowCurrentPassword] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [errorField, setErrorField] = useState<ErrorField>(null)
  const usernameInputRef = useRef<HTMLInputElement>(null)
  const passwordInputRef = useRef<HTMLInputElement>(null)
  const currentPasswordInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!standalone) return
    usernameInputRef.current?.focus()
  }, [standalone])

  const clearError = () => {
    if (error) setError('')
    if (errorField) setErrorField(null)
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (saving || mode === 'deployment') return

    if (mode === 'login') {
      if (!username.trim()) {
        setError(t.webLoginInvalid)
        setErrorField('username')
        usernameInputRef.current?.focus()
        return
      }
      if (!password) {
        setError(t.webLoginInvalid)
        setErrorField('password')
        passwordInputRef.current?.focus()
        return
      }
    } else {
      if (!USERNAME_PATTERN.test(username)) {
        setError(t.webLoginPasswordRules)
        setErrorField('username')
        usernameInputRef.current?.focus()
        return
      }
      if (mode === 'managed' && !currentPassword) {
        setError(t.webLoginCurrentPasswordInvalid)
        setErrorField('currentPassword')
        currentPasswordInputRef.current?.focus()
        return
      }
      if (mode === 'managed' && !password && username === initialUsername) {
        setError(t.webLoginNoChanges)
        setErrorField('username')
        usernameInputRef.current?.focus()
        return
      }
      const passwordRequired = mode === 'setup' || password.length > 0
      if (passwordRequired && (password.length < MIN_WEB_LOGIN_PASSWORD_LENGTH || password.length > 256)) {
        setError(t.webLoginPasswordRules)
        setErrorField('password')
        passwordInputRef.current?.focus()
        return
      }
    }

    setSaving(true)
    setError('')
    setErrorField(null)
    try {
      const result = await saveWebLogin(mode, {
        username,
        password,
        currentPassword: mode === 'managed' ? currentPassword : undefined
      })
      if (result === 204) {
        onSaved()
      } else if (result === 401 || result === 403) {
        setError(mode === 'managed' ? t.webLoginCurrentPasswordInvalid : t.webLoginInvalid)
        setErrorField(mode === 'managed' ? 'currentPassword' : 'password')
      } else if (result === 400) {
        setError(t.webLoginPasswordRules)
        setErrorField('password')
      } else {
        setError(t.webLoginSaveFailed)
      }
    } catch {
      setError(t.webLoginSaveFailed)
    } finally {
      setSaving(false)
    }
  }

  const submitLabel = mode === 'login' ? t.webLoginSignIn : mode === 'setup' ? t.webLoginSetupSubmit : t.saveChanges

  const formContent = (
    <form className="web-login-form" noValidate onSubmit={(event) => void submit(event)}>
      <div className="web-login-field">
        <div className="web-login-field-header">
          <label className="web-login-label" htmlFor="web-login-username">
            {t.username}
          </label>
        </div>
        <Input
          ref={usernameInputRef}
          autoComplete={mode === 'setup' ? 'off' : 'username'}
          disabled={saving}
          id="web-login-username"
          isError={errorField === 'username'}
          maxLength={64}
          onChange={(event) => {
            setUsername(event.target.value)
            clearError()
          }}
          placeholder={t.webLoginUsernamePlaceholder}
          required
          sizeVariant={standalone ? 'lg' : 'md'}
          type="text"
          value={username}
        />
      </div>

      {mode === 'managed' && (
        <div className="web-login-field">
          <div className="web-login-field-header">
            <label className="web-login-label" htmlFor="web-login-current-password">
              {t.webLoginCurrentPassword}
            </label>
          </div>
          <Input
            ref={currentPasswordInputRef}
            autoComplete="current-password"
            disabled={saving}
            id="web-login-current-password"
            isError={errorField === 'currentPassword'}
            maxLength={256}
            onChange={(event) => {
              setCurrentPassword(event.target.value)
              clearError()
            }}
            placeholder={t.webLoginCurrentPasswordPlaceholder}
            required
            sizeVariant="md"
            suffix={
              <PasswordToggleButton
                disabled={saving}
                onToggle={() => setShowCurrentPassword((value) => !value)}
                visible={showCurrentPassword}
              />
            }
            type={showCurrentPassword ? 'text' : 'password'}
            value={currentPassword}
          />
        </div>
      )}

      <div className="web-login-field">
        <div className="web-login-field-header">
          <label className="web-login-label" htmlFor="web-login-password">
            {mode === 'login' ? t.password : t.webLoginNewPassword}
          </label>
          {mode !== 'login' && (
            <span className="web-login-field-meta">
              {mode === 'managed' ? t.webLoginPasswordOptionalHint : t.webLoginPasswordMinHint}
            </span>
          )}
        </div>
        <Input
          ref={passwordInputRef}
          autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
          disabled={saving}
          id="web-login-password"
          isError={errorField === 'password'}
          maxLength={256}
          minLength={mode === 'setup' ? MIN_WEB_LOGIN_PASSWORD_LENGTH : undefined}
          onChange={(event) => {
            setPassword(event.target.value)
            clearError()
          }}
          placeholder={
            mode === 'login'
              ? t.webLoginPasswordPlaceholder
              : mode === 'managed'
                ? t.webLoginKeepPasswordPlaceholder
                : t.webLoginNewPasswordPlaceholder
          }
          required={mode !== 'managed'}
          sizeVariant={standalone ? 'lg' : 'md'}
          suffix={
            <PasswordToggleButton
              disabled={saving}
              onToggle={() => setShowPassword((value) => !value)}
              visible={showPassword}
            />
          }
          type={showPassword ? 'text' : 'password'}
          value={password}
        />
      </div>

      {mode !== 'login' && (
        <p className="web-login-rules">{mode === 'managed' ? t.webLoginManagedRules : t.webLoginPasswordRules}</p>
      )}

      {error && (
        <p className="web-login-error" role="alert">
          {error}
        </p>
      )}

      <div className="web-login-actions">
        <Button
          className={standalone ? 'web-login-submit-full' : undefined}
          loading={saving}
          reserveLoadingSpace
          size={standalone ? 'lg' : 'md'}
          type="submit"
          variant="primary"
        >
          {submitLabel}
        </Button>
      </div>
    </form>
  )

  if (standalone) {
    return (
      <main className="web-login-page">
        <div aria-hidden="true" className="web-login-page__grid" />
        <section className="web-login-card">
          <header className="web-login-card__header">
            <div aria-hidden="true" className="web-login-card__mark">
              <img alt="" className="web-login-card__icon" src={appDefaultIcon} />
            </div>
            <div className="web-login-card__brand">FileTerm</div>
            <h1 className="web-login-card__title">{mode === 'setup' ? t.webLoginSetupTitle : t.webLoginTitle}</h1>
            <p className="web-login-card__subtitle">{mode === 'setup' ? t.webLoginSetupHint : t.webLoginSubtitle}</p>
          </header>
          {formContent}
        </section>
      </main>
    )
  }

  return (
    <div className="settings-panel web-login-settings-panel">
      <section className="settings-section">
        <h3>{t.webLoginSettings}</h3>
        {mode === 'deployment' ? (
          <p className="settings-tools-hint">{t.webLoginDeploymentManaged}</p>
        ) : (
          <>
            {mode === 'setup' && <p className="settings-tools-hint">{t.webLoginSetupHint}</p>}
            {formContent}
          </>
        )}
      </section>
    </div>
  )
}
