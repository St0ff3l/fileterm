import { useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, SecuritySettings } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { FeedbackText } from '../common/FeedbackText'
import { waitForMinimumBusyDuration } from '../common/operation-timing'
import { SessionLockScreen } from './SessionLockScreen'

type SecurityOperation = 'load' | null
type SecurityActionFeedback = {
  target: 'session' | 'backup'
  kind: 'success' | 'error'
  message: string
}

export function SecuritySettingsPanel({
  desktopApi,
  notice,
  focusBackupPasswordRequest = 0,
  onBackupPasswordFocusHandled,
  onBackupPasswordSaved
}: {
  desktopApi?: FileTermDesktopApi
  notice?: string | null
  focusBackupPasswordRequest?: number
  onBackupPasswordFocusHandled?: () => void
  onBackupPasswordSaved?: () => void
}) {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [lockEnabled, setLockEnabled] = useState(false)
  const [idleLockMinutes, setIdleLockMinutes] = useState(0)
  const [lockPassword, setLockPassword] = useState('')
  const [lockPasswordConfirmation, setLockPasswordConfirmation] = useState('')
  const [backupPassword, setBackupPassword] = useState('')
  const [backupPasswordConfirmation, setBackupPasswordConfirmation] = useState('')
  const [operation, setOperation] = useState<SecurityOperation>('load')
  const [isSessionSaving, setIsSessionSaving] = useState(false)
  const [isBackupSaving, setIsBackupSaving] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null)
  const [actionFeedback, setActionFeedback] = useState<SecurityActionFeedback | null>(null)
  const [previewMode, setPreviewMode] = useState<'loading' | 'locked' | null>(null)
  const backupPasswordSectionRef = useRef<HTMLDivElement>(null)
  const backupPasswordInputRef = useRef<HTMLInputElement>(null)

  const applySettings = (next: SecuritySettings) => {
    setSettings(next)
    setLockEnabled(next.lockEnabled)
    setIdleLockMinutes(next.idleLockMinutes)
  }

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

  useEffect(() => {
    if (!desktopApi || focusBackupPasswordRequest <= 0 || operation !== null) return

    const frame = window.requestAnimationFrame(() => {
      backupPasswordSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      backupPasswordInputRef.current?.focus({ preventScroll: true })
      onBackupPasswordFocusHandled?.()
    })

    return () => window.cancelAnimationFrame(frame)
  }, [desktopApi, focusBackupPasswordRequest, onBackupPasswordFocusHandled, operation])

  const saveSessionSettings = async () => {
    if (!desktopApi || operation || isSessionSaving) return
    if (lockPassword && lockPassword !== lockPasswordConfirmation) {
      const message = t.securityPasswordMismatch
      setErrorMessage(message)
      setActionFeedback({ target: 'session', kind: 'error', message })
      return
    }
    if (lockEnabled && !settings?.hasLockPassword && !lockPassword) {
      const message = t.securityLockPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'session', kind: 'error', message })
      return
    }

    const operationStartedAt = performance.now()
    setIsSessionSaving(true)
    setErrorMessage(null)
    setFeedbackMessage(null)
    setActionFeedback(null)
    let saved = false
    try {
      const next = await desktopApi.setSecuritySettings({
        idleLockMinutes,
        lockEnabled,
        ...(lockPassword ? { lockPassword } : {})
      })
      applySettings(next)
      setFeedbackMessage(t.securitySessionSaved)
      setActionFeedback({ target: 'session', kind: 'success', message: t.securitySessionSaved })
      saved = true
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      const displayMessage = message === 'SECURITY_LOCK_PASSWORD_REQUIRED' ? t.securityLockPasswordRequired : message
      setErrorMessage(displayMessage)
      setActionFeedback({ target: 'session', kind: 'error', message: displayMessage })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      if (saved) {
        setLockPassword('')
        setLockPasswordConfirmation('')
      }
      setIsSessionSaving(false)
    }
  }

  const saveBackupPassword = async () => {
    if (!desktopApi || operation || isBackupSaving) return
    if (!backupPassword) {
      const message = t.securityBackupPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
      backupPasswordInputRef.current?.focus()
      return
    }
    if (backupPassword !== backupPasswordConfirmation) {
      const message = t.securityPasswordMismatch
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
      backupPasswordInputRef.current?.focus()
      return
    }

    const operationStartedAt = performance.now()
    setIsBackupSaving(true)
    setErrorMessage(null)
    setFeedbackMessage(null)
    setActionFeedback(null)
    let saved = false
    try {
      const next = await desktopApi.setSecuritySettings({ backupPassword })
      applySettings(next)
      setFeedbackMessage(t.securityBackupPasswordSaved)
      setActionFeedback({ target: 'backup', kind: 'success', message: t.securityBackupPasswordSaved })
      onBackupPasswordSaved?.()
      saved = true
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      if (saved) {
        setBackupPassword('')
        setBackupPasswordConfirmation('')
      }
      setIsBackupSaving(false)
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
  const hasLockPassword = settings?.hasLockPassword === true
  const hasBackupPassword = settings?.hasBackupPassword === true
  const topFeedback = isLoading
    ? { message: t.securityLoading, tone: 'loading' as const }
    : errorMessage && !actionFeedback
      ? { message: errorMessage, tone: 'error' as const }
      : notice
        ? { message: notice, tone: 'info' as const }
        : feedbackMessage && !actionFeedback
          ? { message: feedbackMessage, tone: 'success' as const }
          : null

  return (
    <div className="settings-panel security-settings-panel">
      <section className="settings-section">
        <h3>{t.securitySettings}</h3>
        <p className="settings-tools-hint">{t.securitySettingsHint}</p>

        {topFeedback ? <FeedbackText message={topFeedback.message} tone={topFeedback.tone} /> : null}

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
                disabled={isLoading || isSessionSaving || (!hasLockPassword && !lockPassword)}
                onChange={(event) => {
                  setLockEnabled(event.target.checked)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
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
                disabled={isLoading || isSessionSaving || idleLockMinutes <= 0}
                onClick={() => adjustIdleMinutes(-1)}
                type="button"
              >
                −
              </button>
              <input
                aria-label={t.securityIdleLockMinutes}
                className="security-stepper-input"
                disabled={isLoading || isSessionSaving}
                max={1440}
                min={0}
                onChange={(event) => {
                  const parsed = Number.parseInt(event.target.value, 10)
                  setIdleLockMinutes(Number.isFinite(parsed) ? Math.min(1440, Math.max(0, parsed)) : 0)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
                }}
                type="number"
                value={idleLockMinutes}
              />
              <button
                aria-label={t.securityIncreaseIdleLock}
                className="security-stepper-btn"
                disabled={isLoading || isSessionSaving || idleLockMinutes >= 1440}
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
                disabled={isLoading}
                readOnly={isSessionSaving}
                onChange={(event) => {
                  setLockPassword(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
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
                disabled={isLoading}
                readOnly={isSessionSaving}
                onChange={(event) => {
                  setLockPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
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
              aria-busy={isSessionSaving}
              className="primary-button compact"
              disabled={isLoading || isSessionSaving}
              onClick={() => void saveSessionSettings()}
              type="button"
            >
              <span aria-hidden="true" className="security-action-icon">
                {isSessionSaving ? <span className="button-spinner" /> : <AppIcon name="check" size={14} />}
              </span>
              <span className="security-action-label">
                <span>{isSessionSaving ? t.securitySaving : t.securityApplySession}</span>
                <span aria-hidden="true" className="security-action-label-reserve">
                  {t.securityApplySession}
                </span>
                <span aria-hidden="true" className="security-action-label-reserve">
                  {t.securitySaving}
                </span>
              </span>
            </button>
            <button className="flat-button compact" onClick={() => setPreviewMode('loading')} type="button">
              <AppIcon name="shield-check" size={14} />
              <span>{t.securityPreviewLockScreen}</span>
            </button>
            {actionFeedback?.target === 'session' ? (
              <FeedbackText
                className="security-inline-feedback"
                message={actionFeedback.message}
                tone={actionFeedback.kind}
              />
            ) : null}
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h3>{t.securityBackupCardTitle}</h3>
        <p className="settings-tools-hint">{t.securityBackupCardHint}</p>

        <div className="security-card" ref={backupPasswordSectionRef}>
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
                disabled={isLoading}
                readOnly={isBackupSaving}
                onChange={(event) => {
                  setBackupPassword(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
                }}
                placeholder={t.securityBackupPasswordPlaceholder}
                ref={backupPasswordInputRef}
                type="password"
                value={backupPassword}
              />
            </label>
            <label>
              <span>{t.securityConfirmBackupPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading}
                readOnly={isBackupSaving}
                onChange={(event) => {
                  setBackupPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
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
              aria-busy={isBackupSaving}
              className="primary-button compact"
              disabled={isLoading || isBackupSaving}
              onClick={() => void saveBackupPassword()}
              type="button"
            >
              <span aria-hidden="true" className="security-action-icon">
                {isBackupSaving ? <span className="button-spinner" /> : <AppIcon name="check" size={14} />}
              </span>
              <span className="security-action-label">
                <span>
                  {isBackupSaving
                    ? t.securitySaving
                    : hasBackupPassword
                      ? t.securityReplaceBackupPassword
                      : t.securitySaveBackupPassword}
                </span>
                <span aria-hidden="true" className="security-action-label-reserve">
                  {t.securitySaveBackupPassword}
                </span>
                <span aria-hidden="true" className="security-action-label-reserve">
                  {t.securityReplaceBackupPassword}
                </span>
                <span aria-hidden="true" className="security-action-label-reserve">
                  {t.securitySaving}
                </span>
              </span>
            </button>
            {actionFeedback?.target === 'backup' ? (
              <FeedbackText
                className="security-inline-feedback"
                message={actionFeedback.message}
                tone={actionFeedback.kind}
              />
            ) : null}
          </div>
        </div>
      </section>

      <div className="security-storage-note">
        <AppIcon name="config-file" size={14} />
        <span>{t.securityStorageHint}</span>
      </div>

      {previewMode !== null ? (
        <div
          className="security-lock-preview-overlay"
          style={{
            position: 'fixed',
            inset: 0,
            zIndex: 1000
          }}
        >
          <SessionLockScreen
            mode={previewMode}
            onRetry={() => setPreviewMode('loading')}
            onUnlock={async () => {
              setPreviewMode(null)
              return true
            }}
          />
          <div
            style={{
              position: 'fixed',
              top: 20,
              right: 24,
              zIndex: 1010,
              display: 'flex',
              gap: 8
            }}
          >
            <button
              className="flat-button compact"
              onClick={() => setPreviewMode(previewMode === 'loading' ? 'locked' : 'loading')}
              style={{
                background: 'var(--bg-elevated)',
                color: 'var(--text-main)',
                borderColor: 'var(--border-light)'
              }}
              type="button"
            >
              <AppIcon name="refresh" size={14} />
              <span>
                {previewMode === 'loading' ? t.securitySwitchToLockedPreview : t.securitySwitchToLoadingPreview}
              </span>
            </button>
            <button className="primary-button compact" onClick={() => setPreviewMode(null)} type="button">
              <AppIcon name="close" size={14} />
              <span>{t.securityClosePreview}</span>
            </button>
          </div>
        </div>
      ) : null}
    </div>
  )
}
