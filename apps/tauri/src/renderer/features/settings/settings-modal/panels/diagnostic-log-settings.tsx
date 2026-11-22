import { useEffect, useRef, useState } from 'react'
import {
  DIAGNOSTIC_LOG_CATEGORIES,
  type UiPreferences,
  type UiPreferencesInput,
  type FileTermDesktopApi
} from '@fileterm/core'
import { FeedbackBubble } from '../../../../components/common/feedback-bubble/feedback-bubble'
import type { LocaleMessages } from '../../../../i18n'
import { SelectionControl } from '../../../common/selection-control'

export function DiagnosticLogSettings({
  desktopApi,
  initialPreferences,
  t
}: {
  desktopApi: FileTermDesktopApi | undefined
  initialPreferences?: Pick<UiPreferences, 'basicLoggingEnabled' | 'diagnosticLogCategories'>
  t: LocaleMessages
}) {
  const [preferences, setPreferences] = useState<Pick<
    UiPreferences,
    'basicLoggingEnabled' | 'diagnosticLogCategories'
  > | null>(() => initialPreferences ?? null)
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState(false)
  const [saveError, setSaveError] = useState(false)
  const savingRef = useRef(false)

  useEffect(() => {
    if (!desktopApi) return
    let active = true
    const apply = (next: UiPreferences) => {
      if (active) {
        setPreferences(next)
        setLoadError(false)
      }
    }
    let receivedPreferenceChange = false
    const unsubscribe = desktopApi.onUiPreferencesChanged((next) => {
      receivedPreferenceChange = true
      apply(next)
    })
    setLoadError(false)
    if (!initialPreferences) {
      void desktopApi
        .getUiPreferences()
        .then((next) => {
          if (!receivedPreferenceChange) apply(next)
        })
        .catch(() => {
          if (active) setLoadError(true)
        })
    }
    return () => {
      active = false
      unsubscribe()
    }
  }, [desktopApi, initialPreferences])

  const save = async (input: Pick<UiPreferencesInput, 'basicLoggingEnabled' | 'diagnosticLogCategories'>) => {
    if (!desktopApi || savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setSaveError(false)
    try {
      const next = await desktopApi.setUiPreferences(input)
      setPreferences(next)
    } catch {
      setSaveError(true)
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }

  return (
    <div className="diagnostic-log-settings">
      <label className="diagnostic-log-card diagnostic-log-basic">
        <span className="diagnostic-log-basic-copy">
          <span className="diagnostic-log-card-label">{t.basicLogging}</span>
          <span className="diagnostic-log-basic-hint">{t.basicLoggingHint}</span>
        </span>
        <span className="command-toggle">
          <SelectionControl
            type="checkbox"
            aria-label={t.basicLogging}
            checked={preferences?.basicLoggingEnabled ?? true}
            disabled={!preferences || saving}
            onChange={(event) => void save({ basicLoggingEnabled: event.target.checked })}
          />
        </span>
      </label>
      <div className="diagnostic-log-intro">
        <p>{t.diagnosticLogCategoriesHint}</p>
        <p>{t.diagnosticLogRetentionHint}</p>
      </div>
      <div className="diagnostic-log-grid" aria-busy={saving}>
        {DIAGNOSTIC_LOG_CATEGORIES.map((category) => (
          <label className="diagnostic-log-card" key={category} title={t[`diagnosticLogCategory_${category}`]}>
            <span className="diagnostic-log-card-label">{t[`diagnosticLogCategory_${category}`]}</span>
            <span className="command-toggle">
              <SelectionControl
                type="checkbox"
                checked={preferences?.diagnosticLogCategories[category] ?? false}
                disabled={!preferences || saving}
                onChange={(event) => void save({ diagnosticLogCategories: { [category]: event.target.checked } })}
              />
            </span>
          </label>
        ))}
      </div>
      <p className="diagnostic-log-help">{t.diagnosticLogTroubleshootingHint}</p>
      {loadError ? (
        <p className="modal-error" role="alert">
          {t.diagnosticLogSettingsFailed}
        </p>
      ) : null}
      <FeedbackBubble
        message={saveError ? t.diagnosticLogSettingsFailed : null}
        sourceKey="settings.logs.save"
        tone="error"
      />
    </div>
  )
}
