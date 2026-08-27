import { useEffect, useState } from 'react'
import type { FileTermDesktopApi, SecuritySettings } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'

type SecurityOperation = 'load' | 'session' | 'backup' | null

export function SecuritySettingsPanel({
  desktopApi,
  notice
}: {
  desktopApi?: FileTermDesktopApi
  notice?: string | null
}) {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [lockEnabled, setLockEnabled] = useState(false)
  const [idleLockMinutes, setIdleLockMinutes] = useState(0)
  const [lockPassword, setLockPassword] = useState('')
  const [lockPasswordConfirmation, setLockPasswordConfirmation] = useState('')
  const [backupPassword, setBackupPassword] = useState('')
  const [backupPasswordConfirmation, setBackupPasswordConfirmation] = useState('')
  const [operation, setOperation] = useState<SecurityOperation>('load')
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(notice ?? null)

  const applySettings = (next: SecuritySettings) => {
    setSettings(next)
    setLockEnabled(next.lockEnabled)
    setIdleLockMinutes(next.idleLockMinutes)
  }

  useEffect(() => {
    if (notice) {
      setFeedbackMessage(notice)
    }
  }, [notice])

  useEffect(() => {
    let canceled = false
    setErrorMessage(null)
    if (!desktopApi) {
      setSettings(null)
      setOperation(null)
      return () => {
        canceled = true
      }
    }

    setOperation('load')
    const unsubscribe = desktopApi.onSecuritySettingsChanged((next) => {
      if (!canceled) {
        applySettings(next)
      }
    })
    void desktopApi
      .getSecuritySettings()
      .then((next) => {
        if (!canceled) applySettings(next)
      })
      .catch((cause: unknown) => {
        if (!canceled) setErrorMessage(cause instanceof Error ? cause.message : String(cause))
      })
      .finally(() => {
        if (!canceled) setOperation(null)
      })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi])

  const saveSessionSettings = async () => {
    if (!desktopApi || operation) return
    if (lockPassword && lockPassword !== lockPasswordConfirmation) {
      setErrorMessage(t.securityPasswordMismatch)
      return
    }
    if (lockEnabled && !settings?.hasLockPassword && !lockPassword) {
      setErrorMessage(t.securityLockPasswordRequired)
      return
    }

    setOperation('session')
    setErrorMessage(null)
    setFeedbackMessage(null)
    try {
      const next = await desktopApi.setSecuritySettings({
        idleLockMinutes,
        lockEnabled,
        ...(lockPassword ? { lockPassword } : {})
      })
      applySettings(next)
      setLockPassword('')
      setLockPasswordConfirmation('')
      setFeedbackMessage(t.securitySessionSaved)
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      setErrorMessage(message === 'SECURITY_LOCK_PASSWORD_REQUIRED' ? t.securityLockPasswordRequired : message)
    } finally {
      setOperation(null)
    }
  }

  const saveBackupPassword = async () => {
    if (!desktopApi || operation) return
    if (!backupPassword) {
      setErrorMessage(t.securityBackupPasswordRequired)
      return
    }
    if (backupPassword !== backupPasswordConfirmation) {
      setErrorMessage(t.securityPasswordMismatch)
      return
    }

    setOperation('backup')
    setErrorMessage(null)
    setFeedbackMessage(null)
    try {
      const next = await desktopApi.setSecuritySettings({ backupPassword })
      applySettings(next)
      setBackupPassword('')
      setBackupPasswordConfirmation('')
      setFeedbackMessage(t.securityBackupPasswordSaved)
    } catch (cause) {
      setErrorMessage(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setOperation(null)
    }
  }

  const adjustIdleMinutes = (delta: number) => {
    setIdleLockMinutes((current) => Math.min(1440, Math.max(0, current + delta)))
  }

  if (!desktopApi) {
    return (
      <div className="settings-panel security-settings-panel">
        <section className="settings-section">
          <h3>{t.securitySettings}</h3>
          <p className="settings-tools-hint">{t.securityDesktopOnly}</p>
        </section>
      </div>
    )
  }

  const isLoading = operation === 'load'
  const isSaving = operation === 'session' || operation === 'backup'
  const hasLockPassword = settings?.hasLockPassword === true
  const hasBackupPassword = settings?.hasBackupPassword === true

  return (
    <div className="settings-panel security-settings-panel">
      <section className="settings-section">
        <h3>{t.securitySettings}</h3>
        <p className="settings-tools-hint">{t.securitySettingsHint}</p>

        {isLoading ? (
          <div aria-busy="true" className="settings-feedback-banner settings-feedback-banner--loading">
            <span aria-hidden="true" className="button-spinner" />
            <span>{t.securityLoading}</span>
          </div>
        ) : null}
        {errorMessage ? (
          <p className="modal-error" role="alert">
            {errorMessage}
          </p>
        ) : null}
        {feedbackMessage ? (
          <div className="settings-feedback-banner settings-feedback-banner--success" role="status">
            <AppIcon name="check" size={14} />
            <span>{feedbackMessage}</span>
          </div>
        ) : null}

        <div className="security-card">
          <div className="security-card-header">
            <div className="security-card-header-copy">
              <strong>{t.securityMasterPassword}</strong>
              <p>{t.securityMasterPasswordHint}</p>
            </div>
            <span className={`security-status-badge ${hasLockPassword ? 'configured' : 'muted'}`}>
              {hasLockPassword ? t.securityConfigured : t.securityNotConfigured}
            </span>
          </div>

          <div className="security-preference-row">
            <div className="security-preference-copy">
              <strong>{t.securityEnableLock}</strong>
              <p>{t.securityEnableLockHint}</p>
            </div>
            <label className="ssh-checkbox security-checkbox-wrapper" title={t.securityEnableLock}>
              <input
                checked={lockEnabled}
                disabled={isLoading || isSaving || (!hasLockPassword && !lockPassword)}
                onChange={(event) => {
                  setLockEnabled(event.target.checked)
                  setErrorMessage(null)
                }}
                type="checkbox"
              />
            </label>
          </div>

          <div className="security-preference-row">
            <div className="security-preference-copy">
              <strong>{t.securityIdleLockMinutes}</strong>
              <p>{t.securityIdleLockMinutesHint}</p>
            </div>
            <div className="security-stepper-control" aria-label={t.securityIdleLockMinutes}>
              <button
                aria-label={t.securityDecreaseIdleLock}
                className="security-stepper-btn"
                disabled={isLoading || isSaving || idleLockMinutes <= 0}
                onClick={() => adjustIdleMinutes(-1)}
                type="button"
              >
                −
              </button>
              <input
                aria-label={t.securityIdleLockMinutes}
                className="security-stepper-input"
                disabled={isLoading || isSaving}
                max={1440}
                min={0}
                onChange={(event) => {
                  const parsed = Number.parseInt(event.target.value, 10)
                  setIdleLockMinutes(Number.isFinite(parsed) ? Math.min(1440, Math.max(0, parsed)) : 0)
                }}
                type="number"
                value={idleLockMinutes}
              />
              <button
                aria-label={t.securityIncreaseIdleLock}
                className="security-stepper-btn"
                disabled={isLoading || isSaving || idleLockMinutes >= 1440}
                onClick={() => adjustIdleMinutes(1)}
                type="button"
              >
                <AppIcon name="plus" size={12} />
              </button>
              <span className="security-stepper-unit">{t.securityMinutes}</span>
            </div>
          </div>

          <div className="security-form-grid">
            <label>
              <span>{hasLockPassword ? t.securityChangeMasterPassword : t.securitySetMasterPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSaving}
                onChange={(event) => {
                  setLockPassword(event.target.value)
                  setErrorMessage(null)
                }}
                placeholder={t.securityPasswordPlaceholder}
                type="password"
                value={lockPassword}
              />
            </label>
            <label>
              <span>{t.securityConfirmMasterPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSaving}
                onChange={(event) => {
                  setLockPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                }}
                placeholder={t.securityConfirmPasswordPlaceholder}
                type="password"
                value={lockPasswordConfirmation}
              />
            </label>
          </div>

          <p className="security-policy-hint">{t.securityLockPasswordPolicy}</p>

          <div className="security-actions-row">
            <button
              className="primary-button compact"
              disabled={isLoading || isSaving}
              onClick={() => void saveSessionSettings()}
              type="button"
            >
              {operation === 'session' ? (
                <span aria-hidden="true" className="button-spinner" />
              ) : (
                <AppIcon name="check" size={13} />
              )}
              <span>{t.securityApplySession}</span>
            </button>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h3>{t.securityBackupCardTitle}</h3>
        <p className="settings-tools-hint">{t.securityBackupCardHint}</p>

        <div className="security-card">
          <div className="security-card-header">
            <div className="security-card-header-copy">
              <strong>{t.securityBackupCardTitle}</strong>
              <p>{t.securityBackupCardHint}</p>
            </div>
            <span className={`security-status-badge ${hasBackupPassword ? 'configured' : 'muted'}`}>
              {hasBackupPassword ? t.securityConfigured : t.securityNotConfigured}
            </span>
          </div>

          <div className="security-callout">
            <AppIcon name="shield" size={14} />
            <span>{t.securityBackupPasswordUsage}</span>
          </div>

          <div className="security-form-grid">
            <label>
              <span>{t.securityBackupPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSaving}
                onChange={(event) => {
                  setBackupPassword(event.target.value)
                  setErrorMessage(null)
                }}
                placeholder={t.securityBackupPasswordPlaceholder}
                type="password"
                value={backupPassword}
              />
            </label>
            <label>
              <span>{t.securityConfirmBackupPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSaving}
                onChange={(event) => {
                  setBackupPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                }}
                placeholder={t.securityConfirmPasswordPlaceholder}
                type="password"
                value={backupPasswordConfirmation}
              />
            </label>
          </div>

          <p className="security-policy-hint">{t.securityBackupPasswordPolicy}</p>

          <div className="security-actions-row">
            <button
              className="primary-button compact"
              disabled={isLoading || isSaving}
              onClick={() => void saveBackupPassword()}
              type="button"
            >
              {operation === 'backup' ? (
                <span aria-hidden="true" className="button-spinner" />
              ) : (
                <AppIcon name="check" size={13} />
              )}
              <span>{hasBackupPassword ? t.securityReplaceBackupPassword : t.securitySaveBackupPassword}</span>
            </button>
          </div>
        </div>
      </section>

      <div className="security-storage-note">
        <AppIcon name="config-file" size={14} />
        <span>{t.securityStorageHint}</span>
      </div>
    </div>
  )
}
