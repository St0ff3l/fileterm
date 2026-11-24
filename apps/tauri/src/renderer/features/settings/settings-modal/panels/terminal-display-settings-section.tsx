import { useEffect, useRef, useState } from 'react'
import {
  TERMINAL_LINE_HEIGHT_OPTIONS,
  type FileTermDesktopApi,
  type TerminalFontWeight,
  type TerminalLineHeight,
  type TerminalRenderer
} from '@fileterm/core'
import { FeedbackBubble } from '../../../../components/common/feedback-bubble/feedback-bubble'
import {
  getTerminalFontWeight,
  getTerminalLineHeight,
  setTerminalFontWeight,
  setTerminalLineHeight
} from '../../../../app/font-metrics'
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
  const [lineHeight, setLineHeight] = useState(getTerminalLineHeight)
  const [savingWeight, setSavingWeight] = useState(false)
  const [savingLineHeight, setSavingLineHeight] = useState(false)
  const [weightError, setWeightError] = useState(false)
  const [lineHeightError, setLineHeightError] = useState(false)
  const savingWeightRef = useRef(false)
  const savingLineHeightRef = useRef(false)
  useEffect(
    () =>
      desktopApi?.onUiPreferencesChanged((preferences) => {
        setWeight(preferences.terminalFontWeight)
        setLineHeight(preferences.terminalLineHeight)
      }),
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
  const saveLineHeight = async (nextLineHeight: TerminalLineHeight) => {
    if (!desktopApi || savingLineHeightRef.current) return
    savingLineHeightRef.current = true
    setSavingLineHeight(true)
    setLineHeightError(false)
    try {
      const preferences = await desktopApi.setUiPreferences({ terminalLineHeight: nextLineHeight })
      setLineHeight(preferences.terminalLineHeight)
      setTerminalLineHeight(preferences.terminalLineHeight)
    } catch {
      setLineHeightError(true)
    } finally {
      savingLineHeightRef.current = false
      setSavingLineHeight(false)
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
      <FeedbackBubble
        message={weightError ? t.terminalFontWeightSaveFailed : null}
        sourceKey="settings.terminal.font-weight"
        tone="error"
      />
      <div className="theme-config-control" aria-busy={savingLineHeight}>
        <span className="theme-config-label">{t.terminalLineHeight}</span>
        <DropdownSelect
          ariaLabel={t.terminalLineHeight}
          className="theme-config-select"
          disabled={!desktopApi || savingLineHeight}
          value={String(lineHeight)}
          onChange={(value) => void saveLineHeight(Number(value) as TerminalLineHeight)}
          options={TERMINAL_LINE_HEIGHT_OPTIONS.map((value) => ({
            value: String(value),
            label: `${value.toFixed(2)}×`
          }))}
        />
      </div>
      <p className="settings-tools-hint">{t.terminalLineHeightHint}</p>
      <FeedbackBubble
        message={lineHeightError ? t.terminalLineHeightSaveFailed : null}
        sourceKey="settings.terminal.line-height"
        tone="error"
      />
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
          <FeedbackBubble
            message={terminalRendererPreferenceError}
            sourceKey="settings.terminal.renderer"
            tone="error"
          />
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
      <FeedbackBubble message={terminalZoomPreferenceError} sourceKey="settings.terminal.zoom-lock" tone="error" />
    </section>
  )
}
