import { useEffect, useState } from 'react'
import { SelectionControl } from '../../../common/selection-control'
import { t } from '../../../../i18n'

export function AdvancedSettingsPanel() {
  const [enabled, setEnabled] = useState(true)
  const [loaded, setLoaded] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    const api = window.fileterm
    if (!api) return
    let disposed = false
    let revision = 0
    const apply = (preferences: { terminalCommandCompletion: boolean }) => {
      revision++
      if (!disposed) {
        setEnabled(preferences.terminalCommandCompletion !== false)
        setLoaded(true)
      }
    }
    void api
      .getUiPreferences()
      .then((preferences) => {
        if (revision === 0) apply(preferences)
      })
      .catch((reason: unknown) => {
        if (!disposed) setError(String(reason))
      })
    const off = api.onUiPreferencesChanged(apply)
    return () => {
      disposed = true
      off()
    }
  }, [])
  const save = async (value: boolean) => {
    if (!window.fileterm || saving) return
    setSaving(true)
    setError(null)
    try {
      const preferences = await window.fileterm.setUiPreferences({ terminalCommandCompletion: value })
      setEnabled(preferences.terminalCommandCompletion)
    } catch (reason) {
      setError(String(reason))
    } finally {
      setSaving(false)
    }
  }
  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.advancedSettings}</h3>
        <div className="overview-preference-list" aria-busy={saving}>
          <label className="overview-preference-row">
            <span className="overview-preference-copy">
              <strong>{t.terminalCommandCompletion}</strong>
              <p>{t.terminalCommandCompletionHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                type="checkbox"
                checked={enabled}
                disabled={!loaded || saving}
                onChange={(event) => void save(event.target.checked)}
              />
            </span>
          </label>
        </div>
        {error ? (
          <p className="modal-error" role="alert">
            {error}
          </p>
        ) : null}
      </section>
    </div>
  )
}
