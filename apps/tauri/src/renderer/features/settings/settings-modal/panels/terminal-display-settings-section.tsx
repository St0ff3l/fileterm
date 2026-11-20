import { useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, TerminalFontWeight, TerminalRenderer } from '@fileterm/core'
import { getTerminalFontWeight, setTerminalFontWeight } from '../../../../app/font-metrics'
import { supportsTerminalRenderer } from '../../../../app/terminal-renderer-preference'
import type { LocaleMessages } from '../../../../i18n'
import { DropdownSelect } from '../../../common/dropdown-select'
import { SelectionControl } from '../../../common/selection-control'
import { useSettingsModalContext } from '../context'
import './terminal-display-settings-section.css'

type TerminalDisplaySettingsContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  terminalRenderer: TerminalRenderer
  isSavingTerminalRendererPreference: boolean
  terminalRendererPreferenceError: string | null
  saveTerminalRendererPreference(nextValue: TerminalRenderer): void
  terminalZoomLocked: boolean
  isSavingTerminalZoomPreference: boolean
  terminalZoomPreferenceError: string | null
  setTerminalZoomLockPreference(nextValue: boolean): void
}

/** Renderer selection applies to local and remote terminal surfaces. */
export function TerminalDisplaySettingsSection() {
  const {
    t,
    desktopApi,
    terminalRenderer,
    isSavingTerminalRendererPreference,
    terminalRendererPreferenceError,
    saveTerminalRendererPreference,
    terminalZoomLocked,
    isSavingTerminalZoomPreference,
    terminalZoomPreferenceError,
    setTerminalZoomLockPreference
  } = useSettingsModalContext<TerminalDisplaySettingsContext>()
  const showRendererPreference = supportsTerminalRenderer(desktopApi?.platform)
  const [weight, setWeight] = useState(getTerminalFontWeight)
  const [savingWeight, setSavingWeight] = useState(false)
  const [weightError, setWeightError] = useState(false)
  const savingWeightRef = useRef(false)
  useEffect(
    () => desktopApi?.onUiPreferencesChanged((preferences) => setWeight(preferences.terminalFontWeight)),
    [desktopApi]
  )
  const saveWeight = async (nextWeight: TerminalFontWeight) => {
    if (!desktopApi || savingWeightRef.current) return
    savingWeightRef.current = true
    setSavingWeight(true)
    setWeightError(false)
    try {
      const preferences = await desktopApi.setUiPreferences({ terminalFontWeight: nextWeight })
      setWeight(preferences.terminalFontWeight)
      setTerminalFontWeight(preferences.terminalFontWeight)
    } catch {
      setWeightError(true)
    } finally {
      savingWeightRef.current = false
      setSavingWeight(false)
    }
  }

  return (
    <section className="settings-section terminal-display-settings">
      <h3>{t.terminalDisplaySettings}</h3>
      <div className="theme-config-control" aria-busy={savingWeight}>
        <span className="theme-config-label">{t.terminalFontWeight}</span>
        <DropdownSelect
          ariaLabel={t.terminalFontWeight}
          className="theme-config-select"
          disabled={!desktopApi || savingWeight}
          value={String(weight)}
          onChange={(value) => void saveWeight(Number(value) as TerminalFontWeight)}
          options={[
            { value: '400', label: t.terminalFontWeightNormal },
            { value: '500', label: t.terminalFontWeightMedium },
            { value: '600', label: t.terminalFontWeightSemibold }
          ]}
        />
      </div>
      <p className="settings-tools-hint">{t.terminalFontWeightHint}</p>
      {weightError ? (
        <p className="modal-error" role="alert">
          {t.terminalFontWeightSaveFailed}
        </p>
      ) : null}
      {showRendererPreference ? (
        <>
          <p className="settings-tools-hint">{t.terminalDisplaySettingsHint}</p>
          <div
            aria-busy={isSavingTerminalRendererPreference}
            className="theme-config-control"
            inert={isSavingTerminalRendererPreference}
          >
            <span className="theme-config-label">{t.terminalRenderer}</span>
            <DropdownSelect
              ariaLabel={t.terminalRenderer}
              className="theme-config-select"
              disabled={isSavingTerminalRendererPreference}
              onChange={(value) => saveTerminalRendererPreference(value as TerminalRenderer)}
              options={[
                { value: 'dom', label: 'DOM' },
                { value: 'webgl', label: 'WebGL' }
              ]}
              value={terminalRenderer}
            />
          </div>
          <p className="settings-tools-hint">{t.terminalRendererHint}</p>
          {terminalRendererPreferenceError ? <p className="modal-error">{terminalRendererPreferenceError}</p> : null}
        </>
      ) : null}
      <div
        aria-busy={isSavingTerminalZoomPreference}
        className="overview-preference-list"
        inert={isSavingTerminalZoomPreference}
      >
        <label className="overview-preference-row">
          <span className="overview-preference-copy">
            <strong>{t.lockTerminalZoom}</strong>
            <p>{t.lockTerminalZoomHint}</p>
          </span>
          <span className="command-toggle overview-preference-toggle">
            <SelectionControl
              checked={terminalZoomLocked}
              disabled={!desktopApi}
              onChange={(event) => setTerminalZoomLockPreference(event.target.checked)}
              type="checkbox"
            />
          </span>
        </label>
      </div>
      {terminalZoomPreferenceError ? <p className="modal-error">{terminalZoomPreferenceError}</p> : null}
    </section>
  )
}
