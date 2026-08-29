import { useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, SecuritySettings } from '@fileterm/core'
import { formatMessage, t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { DropdownSelect } from '../common/DropdownSelect'
import { FeedbackText } from '../common/FeedbackText'
import { SelectionControl } from '../common/SelectionControl'
import { waitForMinimumBusyDuration } from '../common/operation-timing'
import { SessionLockScreen } from './SessionLockScreen'

type SecurityOperation = 'load' | null
type SecurityActionFeedback = {
  target: 'session' | 'backup'
  kind: 'success' | 'error'
  message: string
}

const DEFAULT_IDLE_LOCK_MINUTES = 10
const IDLE_LOCK_PRESET_MINUTES = [1, 2, 3, 5, 10, 20, 30, 60, 90, 120, 150, 180, 0] as const

function formatIdleLockOption(minutes: number) {
  if (minutes === 0) {
    return t.securityIdleLockNever
  }

  const hours = Math.floor(minutes / 60)
  const remainingMinutes = minutes % 60
  if (hours === 0) {
    return formatMessage(t.securityIdleLockUnitTemplate, {
      unit: minutes === 1 ? t.securityMinute : t.securityMinutes,
      value: minutes
    })
  }

  const hourLabel = formatMessage(t.securityIdleLockUnitTemplate, {
    unit: hours === 1 ? t.securityHour : t.securityHours,
    value: hours
  })
  if (remainingMinutes === 0) {
    return hourLabel
  }

  const minuteLabel = formatMessage(t.securityIdleLockUnitTemplate, {
    unit: remainingMinutes === 1 ? t.securityMinute : t.securityMinutes,
    value: remainingMinutes
  })
  return formatMessage(t.securityIdleLockCompoundTemplate, {
    hours: hourLabel,
    minutes: minuteLabel
  })
}

function getIdleLockOptions() {
  return IDLE_LOCK_PRESET_MINUTES.map((minutes) => ({
    label: formatIdleLockOption(minutes),
    value: String(minutes)
  }))
}

function normalizeIdleLockMinutes(value: number) {
  return IDLE_LOCK_PRESET_MINUTES.includes(value as (typeof IDLE_LOCK_PRESET_MINUTES)[number])
    ? value
    : DEFAULT_IDLE_LOCK_MINUTES
}

function localizeSecurityError(cause: unknown) {
  const message = cause instanceof Error ? cause.message : String(cause)
  const code = message.replace(/^command error:\s*/, '')
  if (code === 'SECURITY_LOCK_PASSWORD_REQUIRED') return t.securityLockPasswordRequired
  if (code === 'SECURITY_CURRENT_LOCK_PASSWORD_REQUIRED') return t.securityCurrentPasswordRequired
  if (code === 'SECURITY_CURRENT_LOCK_PASSWORD_INVALID') return t.securityCurrentPasswordInvalid
  if (code === 'SECURITY_CURRENT_BACKUP_PASSWORD_REQUIRED') return t.securityCurrentBackupPasswordRequired
  if (code === 'SECURITY_CURRENT_BACKUP_PASSWORD_INVALID') return t.securityCurrentBackupPasswordInvalid
  if (code === 'SECURITY_IDLE_LOCK_MINUTES_INVALID') return t.securityIdleLockMinutesInvalid
  return message
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
  const [idleLockMinutes, setIdleLockMinutes] = useState(DEFAULT_IDLE_LOCK_MINUTES)
  const [currentLockPassword, setCurrentLockPassword] = useState('')
  const [lockResetCurrentPassword, setLockResetCurrentPassword] = useState('')
  const [lockPassword, setLockPassword] = useState('')
  const [lockPasswordConfirmation, setLockPasswordConfirmation] = useState('')
  const [currentBackupPassword, setCurrentBackupPassword] = useState('')
  const [backupPassword, setBackupPassword] = useState('')
  const [backupPasswordConfirmation, setBackupPasswordConfirmation] = useState('')
  const [operation, setOperation] = useState<SecurityOperation>('load')
  const [isSessionSaving, setIsSessionSaving] = useState(false)
  const [isLockResetConfirmOpen, setIsLockResetConfirmOpen] = useState(false)
  const [isLockResetting, setIsLockResetting] = useState(false)
  const [lockResetErrorMessage, setLockResetErrorMessage] = useState<string | null>(null)
  const [isBackupSaving, setIsBackupSaving] = useState(false)
  const [isBackupResetConfirmOpen, setIsBackupResetConfirmOpen] = useState(false)
  const [isBackupResetting, setIsBackupResetting] = useState(false)
  const [backupResetErrorMessage, setBackupResetErrorMessage] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null)
  const [actionFeedback, setActionFeedback] = useState<SecurityActionFeedback | null>(null)
  const [previewMode, setPreviewMode] = useState<'loading' | 'locked' | null>(null)
  const backupPasswordSectionRef = useRef<HTMLDivElement>(null)
  const backupPasswordInputRef = useRef<HTMLInputElement>(null)
  const isSecurityMutationInFlight = isSessionSaving || isLockResetting || isBackupSaving || isBackupResetting
  const idleLockOptions = getIdleLockOptions()

  const applySettings = (next: SecuritySettings) => {
    setSettings(next)
    setLockEnabled(next.lockEnabled)
    setIdleLockMinutes(normalizeIdleLockMinutes(next.idleLockMinutes))
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
    if (!desktopApi || operation || isSecurityMutationInFlight) return

    const hasPendingPasswordInput = Boolean(lockPassword || lockPasswordConfirmation || currentLockPassword)
    if (hasPendingPasswordInput && !lockPassword) {
      const message = t.securityNewPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'session', kind: 'error', message })
      return
    }
    if (lockPassword && !lockPasswordConfirmation) {
      const message = t.securityPasswordConfirmationRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'session', kind: 'error', message })
      return
    }
    if (lockPassword && settings?.hasLockPassword && !currentLockPassword) {
      const message = t.securityCurrentPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'session', kind: 'error', message })
      return
    }
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
        ...(currentLockPassword ? { currentLockPassword } : {}),
        ...(lockPassword ? { lockPassword } : {})
      })
      applySettings(next)
      setFeedbackMessage(t.securitySessionSaved)
      setActionFeedback({ target: 'session', kind: 'success', message: t.securitySessionSaved })
      saved = true
    } catch (cause) {
      const displayMessage = localizeSecurityError(cause)
      setErrorMessage(displayMessage)
      setActionFeedback({ target: 'session', kind: 'error', message: displayMessage })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      if (saved) {
        setCurrentLockPassword('')
        setLockPassword('')
        setLockPasswordConfirmation('')
      }
      setIsSessionSaving(false)
    }
  }

  const saveBackupPassword = async () => {
    if (!desktopApi || operation || isSecurityMutationInFlight) return
    const hasExistingBackupPassword = settings?.hasBackupPassword === true
    const hasPendingPasswordInput = Boolean(currentBackupPassword || backupPassword || backupPasswordConfirmation)
    if (hasPendingPasswordInput && !backupPassword) {
      const message = t.securityNewBackupPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
      backupPasswordInputRef.current?.focus()
      return
    }
    if (!backupPassword) {
      const message = hasExistingBackupPassword ? t.securityNewBackupPasswordRequired : t.securityBackupPasswordRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
      backupPasswordInputRef.current?.focus()
      return
    }
    if (!backupPasswordConfirmation) {
      const message = t.securityBackupPasswordConfirmationRequired
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
      backupPasswordInputRef.current?.focus()
      return
    }
    if (hasExistingBackupPassword && !currentBackupPassword) {
      const message = t.securityCurrentBackupPasswordRequired
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
      const next = await desktopApi.setSecuritySettings({
        ...(currentBackupPassword ? { currentBackupPassword } : {}),
        backupPassword
      })
      applySettings(next)
      setFeedbackMessage(t.securityBackupPasswordSaved)
      setActionFeedback({ target: 'backup', kind: 'success', message: t.securityBackupPasswordSaved })
      onBackupPasswordSaved?.()
      saved = true
    } catch (cause) {
      const message = localizeSecurityError(cause)
      setErrorMessage(message)
      setActionFeedback({ target: 'backup', kind: 'error', message })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      if (saved) {
        setCurrentBackupPassword('')
        setBackupPassword('')
        setBackupPasswordConfirmation('')
      }
      setIsBackupSaving(false)
    }
  }

  const removeSessionLockPassword = async () => {
    if (!desktopApi || isSecurityMutationInFlight) return
    if (!lockResetCurrentPassword) {
      setLockResetErrorMessage(t.securityCurrentPasswordRequired)
      return
    }

    const operationStartedAt = performance.now()
    setIsLockResetting(true)
    setErrorMessage(null)
    setFeedbackMessage(null)
    setActionFeedback(null)
    setLockResetErrorMessage(null)
    try {
      const next = await desktopApi.setSecuritySettings({
        clearLockPassword: true,
        currentLockPassword: lockResetCurrentPassword,
        lockEnabled: false
      })
      applySettings(next)
      setCurrentLockPassword('')
      setLockResetCurrentPassword('')
      setLockPassword('')
      setLockPasswordConfirmation('')
      setIsLockResetConfirmOpen(false)
      setFeedbackMessage(t.securityLockPasswordRemoved)
      setActionFeedback({ target: 'session', kind: 'success', message: t.securityLockPasswordRemoved })
    } catch (cause) {
      setLockResetErrorMessage(localizeSecurityError(cause))
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      setIsLockResetting(false)
    }
  }

  const resetBackupPassword = async () => {
    if (!desktopApi || isSecurityMutationInFlight) return

    const operationStartedAt = performance.now()
    setIsBackupResetting(true)
    setErrorMessage(null)
    setFeedbackMessage(null)
    setActionFeedback(null)
    setBackupResetErrorMessage(null)
    try {
      const next = await desktopApi.resetSecurityBackupPassword()
      applySettings(next)
      setCurrentBackupPassword('')
      setBackupPassword('')
      setBackupPasswordConfirmation('')
      setIsBackupResetConfirmOpen(false)
      setFeedbackMessage(t.securityBackupPasswordReset)
      setActionFeedback({ target: 'backup', kind: 'success', message: t.securityBackupPasswordReset })
    } catch {
      setBackupResetErrorMessage(t.securityResetBackupPasswordFailed)
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      setIsBackupResetting(false)
    }
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
              <SelectionControl
                checked={lockEnabled}
                disabled={isLoading || isSecurityMutationInFlight || (!hasLockPassword && !lockPassword)}
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

          {lockEnabled ? (
            <div className="security-preference-row">
              <div className="security-preference-copy">
                <strong>{t.securityIdleLockMinutes}</strong>
                <p>{t.securityIdleLockMinutesHint}</p>
              </div>
              <DropdownSelect
                ariaLabel={t.securityIdleLockMinutes}
                className="security-idle-lock-select"
                disabled={isLoading || isSecurityMutationInFlight}
                onChange={(value) => {
                  setIdleLockMinutes(normalizeIdleLockMinutes(Number.parseInt(value, 10)))
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
                }}
                options={idleLockOptions}
                value={String(normalizeIdleLockMinutes(idleLockMinutes))}
              />
            </div>
          ) : null}

          <div
            className={`security-form-grid security-password-grid${hasLockPassword ? ' security-password-grid--changing' : ''}`}
          >
            {hasLockPassword ? (
              <label>
                <span>{t.securityCurrentMasterPassword}</span>
                <input
                  autoComplete="current-password"
                  disabled={isLoading || isSecurityMutationInFlight}
                  readOnly={isSessionSaving}
                  onChange={(event) => {
                    setCurrentLockPassword(event.target.value)
                    setErrorMessage(null)
                    setFeedbackMessage(null)
                    setActionFeedback(null)
                  }}
                  placeholder={t.securityCurrentPasswordPlaceholder}
                  type="password"
                  value={currentLockPassword}
                />
              </label>
            ) : null}
            <label>
              <span>{hasLockPassword ? t.securityNewMasterPassword : t.securitySetMasterPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSecurityMutationInFlight}
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
              <span>{hasLockPassword ? t.securityConfirmNewMasterPassword : t.securityConfirmMasterPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSecurityMutationInFlight}
                readOnly={isSessionSaving}
                onChange={(event) => {
                  setLockPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
                }}
                placeholder={
                  hasLockPassword ? t.securityConfirmNewPasswordPlaceholder : t.securityConfirmPasswordPlaceholder
                }
                type="password"
                value={lockPasswordConfirmation}
              />
            </label>
          </div>

          <p className="security-policy-hint">
            {hasLockPassword ? t.securityChangeMasterPasswordHint : t.securityLockPasswordPolicy}
          </p>

          <div className="security-actions-row">
            <button
              aria-busy={isSessionSaving}
              className="primary-button compact"
              disabled={isLoading || isSecurityMutationInFlight}
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
            <button
              className="flat-button compact"
              disabled={isLoading || isSecurityMutationInFlight}
              onClick={() => setPreviewMode('loading')}
              type="button"
            >
              <AppIcon name="shield-check" size={14} />
              <span>{t.securityPreviewLockScreen}</span>
            </button>
            {hasLockPassword ? (
              <button
                className="flat-button security-reset-password-button danger"
                disabled={isLoading || isSecurityMutationInFlight}
                onClick={() => {
                  setLockResetCurrentPassword('')
                  setLockResetErrorMessage(null)
                  setIsLockResetConfirmOpen(true)
                }}
                type="button"
              >
                <AppIcon name="refresh" size={14} />
                <span>{t.securityRemoveLockPassword}</span>
              </button>
            ) : null}
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

          <div
            className={`security-form-grid security-password-grid${hasBackupPassword ? ' security-password-grid--changing' : ''}`}
          >
            {hasBackupPassword ? (
              <label>
                <span>{t.securityCurrentBackupPassword}</span>
                <input
                  autoComplete="current-password"
                  disabled={isLoading || isSecurityMutationInFlight}
                  readOnly={isBackupSaving}
                  onChange={(event) => {
                    setCurrentBackupPassword(event.target.value)
                    setErrorMessage(null)
                    setFeedbackMessage(null)
                    setActionFeedback(null)
                  }}
                  placeholder={t.securityCurrentBackupPasswordPlaceholder}
                  type="password"
                  value={currentBackupPassword}
                />
              </label>
            ) : null}
            <label>
              <span>{hasBackupPassword ? t.securityNewBackupPassword : t.securityBackupPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSecurityMutationInFlight}
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
              <span>{hasBackupPassword ? t.securityConfirmNewBackupPassword : t.securityConfirmBackupPassword}</span>
              <input
                autoComplete="new-password"
                disabled={isLoading || isSecurityMutationInFlight}
                readOnly={isBackupSaving}
                onChange={(event) => {
                  setBackupPasswordConfirmation(event.target.value)
                  setErrorMessage(null)
                  setFeedbackMessage(null)
                  setActionFeedback(null)
                }}
                placeholder={
                  hasBackupPassword ? t.securityConfirmNewPasswordPlaceholder : t.securityConfirmPasswordPlaceholder
                }
                type="password"
                value={backupPasswordConfirmation}
              />
            </label>
          </div>

          <p className="security-policy-hint">
            {hasBackupPassword ? t.securityChangeBackupPasswordHint : t.securityBackupPasswordPolicy}
          </p>

          <div className="security-actions-row">
            <button
              aria-busy={isBackupSaving}
              className="primary-button compact"
              disabled={isLoading || isSecurityMutationInFlight}
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
            {hasBackupPassword ? (
              <button
                className="flat-button security-reset-password-button danger"
                disabled={isLoading || isSecurityMutationInFlight}
                onClick={() => {
                  setBackupResetErrorMessage(null)
                  setIsBackupResetConfirmOpen(true)
                }}
                type="button"
              >
                <AppIcon name="refresh" size={14} />
                <span>{t.securityForgotBackupPassword}</span>
              </button>
            ) : null}
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

      {isLockResetConfirmOpen ? (
        <ConfirmActionDialog
          className="security-lock-reset-dialog"
          confirmDisabled={!lockResetCurrentPassword}
          confirmLabel={t.securityRemoveLockPasswordConfirm}
          description={
            <div className="security-remove-lock-dialog-content">
              <p>{t.securityRemoveLockPasswordDescription}</p>
              <label className="security-remove-lock-password-field">
                <span>{t.securityCurrentMasterPassword}</span>
                <input
                  autoComplete="current-password"
                  autoFocus
                  disabled={isLockResetting}
                  onChange={(event) => {
                    setLockResetCurrentPassword(event.target.value)
                    setLockResetErrorMessage(null)
                  }}
                  placeholder={t.securityCurrentPasswordPlaceholder}
                  type="password"
                  value={lockResetCurrentPassword}
                />
              </label>
            </div>
          }
          errorMessage={lockResetErrorMessage}
          initialFocus="none"
          isSubmitting={isLockResetting}
          onClose={() => {
            if (!isLockResetting) {
              setIsLockResetConfirmOpen(false)
              setLockResetCurrentPassword('')
              setLockResetErrorMessage(null)
            }
          }}
          onConfirm={() => void removeSessionLockPassword()}
          title={t.securityRemoveLockPasswordTitle}
        />
      ) : null}

      {isBackupResetConfirmOpen ? (
        <ConfirmActionDialog
          className="security-backup-reset-dialog"
          confirmLabel={t.securityResetBackupPasswordConfirm}
          description={
            <div>
              <p>{t.securityResetBackupPasswordDescription}</p>
            </div>
          }
          errorMessage={backupResetErrorMessage}
          initialFocus="cancel"
          isSubmitting={isBackupResetting}
          onClose={() => {
            if (!isBackupResetting) {
              setIsBackupResetConfirmOpen(false)
              setBackupResetErrorMessage(null)
            }
          }}
          onConfirm={() => void resetBackupPassword()}
          title={t.securityResetBackupPasswordTitle}
        />
      ) : null}

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
