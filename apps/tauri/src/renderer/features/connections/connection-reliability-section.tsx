import type { CreateProfileInput, SshConnectionDefaults } from '@fileterm/core'
import { t } from '../../i18n'
import { SelectionControl } from '../common/selection-control'
import {
  effectiveConnectionSetting,
  type ConnectionFormSetter,
  type SshConnectionSettingKey
} from './connection-modal-utils'

export function ConnectionReliabilitySection({
  connectionDefaults,
  form,
  setForm,
  setSshConnectionSetting
}: {
  connectionDefaults: SshConnectionDefaults
  form: CreateProfileInput
  setForm: ConnectionFormSetter
  setSshConnectionSetting<K extends SshConnectionSettingKey>(key: K, value: SshConnectionDefaults[K]): void
}) {
  const isSsh = form.type === 'ssh'
  const reconnectMode = isSsh
    ? effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode')
    : (form.reconnectMode ?? 'none')

  const setReconnectMode = (mode: 'none' | 'enter' | 'auto') => {
    if (isSsh) {
      setSshConnectionSetting('reconnectMode', mode)
    } else {
      setForm((prev) => ({ ...prev, reconnectMode: mode }))
    }
  }

  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset">
        <legend>{t.reliability}</legend>
        <div className="reconnect-mode-group" style={{ marginTop: 0 }}>
          <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
          <div className="advanced-toggle-list">
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={reconnectMode === 'none'}
                  name="connection-reconnect-mode"
                  type="radio"
                  onChange={() => setReconnectMode('none')}
                />
                <span className="advanced-toggle-name">{t.reconnectNone}</span>
              </label>
              <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={reconnectMode === 'enter'}
                  name="connection-reconnect-mode"
                  type="radio"
                  onChange={() => setReconnectMode('enter')}
                />
                <span className="advanced-toggle-name">{t.reconnectEnter}</span>
              </label>
              <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={reconnectMode === 'auto'}
                  name="connection-reconnect-mode"
                  type="radio"
                  onChange={() => setReconnectMode('auto')}
                />
                <span className="advanced-toggle-name">{t.autoReconnect}</span>
              </label>
              <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
              {reconnectMode === 'auto' ? (
                <div className="reconnect-auto-limit-box">
                  <label className="reconnect-auto-limit-label">
                    <span>{t.reconnectMaxAttempts}:</span>
                    <input
                      inputMode="numeric"
                      min={0}
                      max={4294967295}
                      type="number"
                      value={form.reconnectMaxAttempts ?? 0}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          reconnectMaxAttempts: Math.max(0, Math.min(4294967295, Number(event.target.value) || 0))
                        }))
                      }
                    />
                  </label>
                  <span className="reconnect-auto-limit-hint">{t.reconnectMaxAttemptsHint}</span>
                </div>
              ) : null}
            </div>
          </div>
        </div>
        <div className="reconnect-mode-group">
          <div className="reconnect-mode-group__label">{t.timeoutAndKeepalive}</div>
          <div className="advanced-toggle-list">
            <div className="advanced-toggle-row">
              <div className="reliability-inputs-grid">
                <label className="reliability-field-box">
                  <span className="reliability-field-label">{t.connectionTimeout}:</span>
                  <input
                    inputMode="numeric"
                    min={5}
                    max={300}
                    type="number"
                    value={form.connectTimeoutSeconds ?? 30}
                    onChange={(event) =>
                      setForm((prev) => ({
                        ...prev,
                        connectTimeoutSeconds: Math.max(5, Math.min(300, Number(event.target.value) || 5))
                      }))
                    }
                  />
                </label>
                <label className="reliability-field-box">
                  <span className="reliability-field-label">{t.operationTimeout}:</span>
                  <input
                    inputMode="numeric"
                    min={5}
                    max={3600}
                    type="number"
                    value={form.operationTimeoutSeconds ?? 60}
                    onChange={(event) =>
                      setForm((prev) => ({
                        ...prev,
                        operationTimeoutSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                      }))
                    }
                  />
                </label>
              </div>
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={form.keepaliveEnabled !== false}
                  type="checkbox"
                  onChange={(event) => setForm((prev) => ({ ...prev, keepaliveEnabled: event.target.checked }))}
                />
                <span className="advanced-toggle-name">{t.keepalive}</span>
              </label>
              <p className="advanced-toggle-hint">{t.keepaliveHint}</p>
              {form.keepaliveEnabled !== false ? (
                <div className="reliability-inputs-grid">
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.keepaliveInterval}:</span>
                    <input
                      inputMode="numeric"
                      min={5}
                      max={3600}
                      type="number"
                      value={form.keepaliveIntervalSeconds ?? 30}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          keepaliveIntervalSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                        }))
                      }
                    />
                  </label>
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.keepaliveMaxMisses}:</span>
                    <input
                      inputMode="numeric"
                      min={1}
                      max={32}
                      type="number"
                      value={form.keepaliveMaxMisses ?? 3}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          keepaliveMaxMisses: Math.max(1, Math.min(32, Number(event.target.value) || 1))
                        }))
                      }
                    />
                  </label>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </fieldset>
    </div>
  )
}
