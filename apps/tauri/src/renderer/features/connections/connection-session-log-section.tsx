import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { SelectionControl } from '../common/selection-control'
import { StableButtonContent } from '../common/stable-button-content'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionSessionLogSection({
  chooseSessionLogDirectory,
  form,
  isSelectingSessionLogDirectory,
  setForm
}: {
  chooseSessionLogDirectory(): Promise<void>
  form: CreateProfileInput
  isSelectingSessionLogDirectory: boolean
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset narrow">
        <legend>{t.sessionLogs}</legend>
        <div className="ssh-grid single">
          <label className="ssh-checkbox advanced-toggle-label">
            <SelectionControl
              checked={form.sessionLogEnabled === true}
              onChange={(event) => setForm((previous) => ({ ...previous, sessionLogEnabled: event.target.checked }))}
              type="checkbox"
            />
            <span className="advanced-toggle-name">{t.autoSaveSessionLog}</span>
          </label>
          <p className="ssh-field-hint session-log-top-hint">{t.autoSaveSessionLogHint}</p>
          {form.sessionLogEnabled === true ? (
            <div className="terminal-key-box session-log-box">
              <strong>{t.sessionLogDirectory}:</strong>
              <div className="session-log-directory-control">
                <input
                  readOnly
                  placeholder={t.sessionLogDefaultDirectory}
                  spellCheck={false}
                  value={form.sessionLogDirectory ?? ''}
                />
                <button
                  aria-busy={isSelectingSessionLogDirectory}
                  className="flat-button"
                  disabled={isSelectingSessionLogDirectory}
                  onClick={() => void chooseSessionLogDirectory()}
                  type="button"
                >
                  <StableButtonContent
                    busy={isSelectingSessionLogDirectory}
                    busyLabel={t.choosingDirectory}
                    label={t.chooseDirectory}
                  />
                </button>
                {form.sessionLogDirectory ? (
                  <button
                    className="flat-button"
                    onClick={() => setForm((previous) => ({ ...previous, sessionLogDirectory: '' }))}
                    type="button"
                  >
                    {t.clear}
                  </button>
                ) : null}
              </div>
              <p className="ssh-field-hint" style={{ margin: 0 }}>
                {t.sessionLogPrivacyHint}
              </p>
              {form.type === 'serial' ? (
                <div className="session-log-serial-options">
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.sessionLogIncludeInput === true}
                      onChange={(event) =>
                        setForm((previous) => ({
                          ...previous,
                          sessionLogIncludeInput: event.target.checked
                        }))
                      }
                      type="checkbox"
                    />
                    <span className="advanced-toggle-name">{t.serialSessionLogIncludeInput}</span>
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.sessionLogTimestamps === true}
                      onChange={(event) =>
                        setForm((previous) => ({
                          ...previous,
                          sessionLogTimestamps: event.target.checked
                        }))
                      }
                      type="checkbox"
                    />
                    <span className="advanced-toggle-name">{t.serialSessionLogTimestamps}</span>
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.sessionLogRaw === true}
                      onChange={(event) =>
                        setForm((previous) => ({ ...previous, sessionLogRaw: event.target.checked }))
                      }
                      type="checkbox"
                    />
                    <span className="advanced-toggle-name">{t.serialSessionLogRaw}</span>
                  </label>
                  <p className="ssh-field-hint" style={{ margin: 0 }}>
                    {t.serialSessionLogOptionsHint}
                  </p>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      </fieldset>
    </div>
  )
}
