import { useState, type FormEvent } from 'react'
import { saveWebLogin, type WebLoginStatus } from '../../../../../bridge/web-login'
import { Input } from '../../../../components/common/input/input'
import { t } from '../../../../i18n'
import './web-login-settings-panel.css'

export function WebLoginSettingsPanel({ status, onSaved }: { status: WebLoginStatus; onSaved(): void }) {
  const { mode } = status
  const [username, setUsername] = useState('username' in status ? status.username : '')
  const [password, setPassword] = useState('')
  const [currentPassword, setCurrentPassword] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (saving || mode === 'deployment') return
    setSaving(true)
    setError('')
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
      } else if (result === 400) {
        setError(t.webLoginPasswordRules)
      } else {
        setError(t.webLoginSaveFailed)
      }
    } catch {
      setError(t.webLoginSaveFailed)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.webLoginSettings}</h3>
        {mode === 'deployment' ? (
          <p className="settings-tools-hint">{t.webLoginDeploymentManaged}</p>
        ) : (
          <form className="web-login-form" onSubmit={(event) => void submit(event)}>
            {mode === 'setup' && <p className="settings-tools-hint">{t.webLoginSetupHint}</p>}
            <div className="web-login-field">
              <label htmlFor="web-login-username">{t.username}</label>
              <Input
                id="web-login-username"
                autoComplete="username"
                disabled={saving}
                maxLength={64}
                onChange={(event) => setUsername(event.target.value)}
                required
                type="text"
                value={username}
              />
            </div>
            {mode === 'managed' && (
              <div className="web-login-field">
                <label htmlFor="web-login-current-password">{t.webLoginCurrentPassword}</label>
                <Input
                  id="web-login-current-password"
                  autoComplete="current-password"
                  disabled={saving}
                  maxLength={256}
                  onChange={(event) => setCurrentPassword(event.target.value)}
                  required
                  type="password"
                  value={currentPassword}
                />
              </div>
            )}
            <div className="web-login-field">
              <label htmlFor="web-login-password">{mode === 'login' ? t.password : t.webLoginNewPassword}</label>
              <Input
                id="web-login-password"
                autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
                disabled={saving}
                maxLength={256}
                minLength={mode === 'login' ? undefined : 12}
                onChange={(event) => setPassword(event.target.value)}
                required
                type="password"
                value={password}
              />
            </div>
            {mode !== 'login' && <p className="settings-tools-hint">{t.webLoginPasswordRules}</p>}
            {error && <p role="alert">{error}</p>}
            <button className="primary-button compact" disabled={saving} type="submit">
              {mode === 'login' ? t.webLoginSignIn : t.saveChanges}
            </button>
          </form>
        )}
      </section>
    </div>
  )
}
