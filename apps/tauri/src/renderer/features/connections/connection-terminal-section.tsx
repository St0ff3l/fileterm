import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { SelectionControl } from '../common/selection-control'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionTerminalSection({
  form,
  isNetworkDevice,
  setForm
}: {
  form: CreateProfileInput
  isNetworkDevice: boolean
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset narrow">
        <legend>{t.terminal}</legend>
        <div className="ssh-grid single">
          <label>
            {t.characterEncoding}:
            <DropdownSelect
              value={form.encoding ?? 'UTF-8'}
              options={[
                { value: 'UTF-8', label: 'UTF-8' },
                { value: 'GBK', label: 'GBK' }
              ]}
              onChange={(value) => setForm((prev) => ({ ...prev, encoding: value }))}
            />
          </label>
          {form.type === 'serial' ? (
            <>
              <label>
                {t.serialNewline}:
                <DropdownSelect
                  value={form.newlineMode ?? 'none'}
                  options={[
                    { value: 'none', label: t.serialNewlineNone },
                    { value: 'lf', label: t.serialNewlineLf },
                    { value: 'cr', label: t.serialNewlineCr },
                    { value: 'crlf', label: t.serialNewlineCrlf }
                  ]}
                  onChange={(value) =>
                    setForm((prev) => ({
                      ...prev,
                      newlineMode: value as CreateProfileInput['newlineMode']
                    }))
                  }
                />
              </label>
              <label>
                {t.serialInputMode}:
                <DropdownSelect
                  value={form.inputMode ?? 'text'}
                  options={[
                    { value: 'text', label: t.serialTextMode },
                    { value: 'hex', label: t.serialHexMode }
                  ]}
                  onChange={(value) =>
                    setForm((prev) => ({ ...prev, inputMode: value as CreateProfileInput['inputMode'] }))
                  }
                />
              </label>
              <div className="advanced-toggle-row serial-local-echo-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.lineMode === true}
                    type="checkbox"
                    onChange={(event) => setForm((prev) => ({ ...prev, lineMode: event.target.checked }))}
                  />
                  <span className="advanced-toggle-name">{t.serialLineMode}</span>
                </label>
                <p className="advanced-toggle-hint">{t.serialLineModeHint}</p>
              </div>
              <label>
                {t.serialOutputMode}:
                <DropdownSelect
                  value={form.outputMode ?? 'text'}
                  options={[
                    { value: 'text', label: t.serialTextMode },
                    { value: 'hex', label: t.serialHexMode }
                  ]}
                  onChange={(value) =>
                    setForm((prev) => ({ ...prev, outputMode: value as CreateProfileInput['outputMode'] }))
                  }
                />
              </label>
              <div className="advanced-toggle-row serial-local-echo-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.localEcho === true}
                    type="checkbox"
                    onChange={(event) => setForm((prev) => ({ ...prev, localEcho: event.target.checked }))}
                  />
                  <span className="advanced-toggle-name">{t.serialLocalEcho}</span>
                </label>
                <p className="advanced-toggle-hint">{t.serialLocalEchoHint}</p>
              </div>
              <div className="serial-line-control-box">
                <div className="reconnect-mode-group__label">{t.serialControlStatus}</div>
                <div className="advanced-toggle-list">
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.dtrOnOpen !== false}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, dtrOnOpen: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.serialDtrOnOpen}</span>
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.rtsOnOpen === true}
                      disabled={form.flowControl === 'hardware' || form.rs485Mode === 'half-duplex'}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, rtsOnOpen: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.serialRtsOnOpen}</span>
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.dtrOnClose === true}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, dtrOnClose: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.serialDtrOnClose}</span>
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.rtsOnClose === true}
                      disabled={form.flowControl === 'hardware' || form.rs485Mode === 'half-duplex'}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, rtsOnClose: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.serialRtsOnClose}</span>
                  </label>
                </div>
                <p className="advanced-toggle-hint">{t.serialLineControlHint}</p>
              </div>
              <label>
                {t.serialCharDelay}:
                <input
                  inputMode="numeric"
                  min={0}
                  max={60000}
                  type="number"
                  value={form.serialCharDelayMs ?? 0}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialCharDelayMs: Math.max(0, Math.min(60000, Number(event.target.value) || 0))
                    }))
                  }
                />
              </label>
              <label>
                {t.serialLineDelay}:
                <input
                  inputMode="numeric"
                  min={0}
                  max={60000}
                  type="number"
                  value={form.serialLineDelayMs ?? 0}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialLineDelayMs: Math.max(0, Math.min(60000, Number(event.target.value) || 0))
                    }))
                  }
                />
              </label>
              <p className="ssh-field-hint span-2">{t.serialPacingHint}</p>
              <label>
                {t.serialReceiveIdleTimeout}:
                <input
                  inputMode="numeric"
                  min={250}
                  max={600000}
                  type="number"
                  value={form.serialReceiveIdleTimeoutMs ?? 5000}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialReceiveIdleTimeoutMs: Math.max(250, Math.min(600000, Number(event.target.value) || 250))
                    }))
                  }
                />
              </label>
              <label>
                {t.serialWriteTimeoutSetting}:
                <input
                  inputMode="numeric"
                  min={250}
                  max={600000}
                  type="number"
                  value={form.serialWriteTimeoutMs ?? 30000}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialWriteTimeoutMs: Math.max(250, Math.min(600000, Number(event.target.value) || 250))
                    }))
                  }
                />
              </label>
              <p className="ssh-field-hint span-2">{t.serialTimeoutHint}</p>
              <label>
                {t.serialTransferMaxFileBytes}:
                <input
                  inputMode="numeric"
                  min={1024 * 1024}
                  max={64 * 1024 * 1024 * 1024}
                  type="number"
                  value={form.serialTransferMaxFileBytes ?? 4 * 1024 * 1024 * 1024}
                  onChange={(event) =>
                    setForm((prev) => {
                      const value = Math.max(
                        1024 * 1024,
                        Math.min(64 * 1024 * 1024 * 1024, Number(event.target.value) || 1024 * 1024)
                      )
                      return {
                        ...prev,
                        serialTransferMaxFileBytes: value,
                        serialTransferMaxBatchBytes: Math.max(prev.serialTransferMaxBatchBytes ?? 0, value)
                      }
                    })
                  }
                />
              </label>
              <label>
                {t.serialTransferMaxBatchBytes}:
                <input
                  inputMode="numeric"
                  min={form.serialTransferMaxFileBytes ?? 1024 * 1024}
                  max={256 * 1024 * 1024 * 1024}
                  type="number"
                  value={form.serialTransferMaxBatchBytes ?? 16 * 1024 * 1024 * 1024}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialTransferMaxBatchBytes: Math.max(
                        prev.serialTransferMaxFileBytes ?? 1024 * 1024,
                        Math.min(256 * 1024 * 1024 * 1024, Number(event.target.value) || 1024 * 1024)
                      )
                    }))
                  }
                />
              </label>
              <label>
                {t.serialTransferMaxFiles}:
                <input
                  inputMode="numeric"
                  min={1}
                  max={4096}
                  type="number"
                  value={form.serialTransferMaxFiles ?? 128}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      serialTransferMaxFiles: Math.max(1, Math.min(4096, Number(event.target.value) || 1))
                    }))
                  }
                />
              </label>
              <p className="ssh-field-hint span-2">{t.serialTransferLimitsHint}</p>
              <div className="reconnect-mode-group serial-reconnect-mode-group">
                <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
                <div className="advanced-toggle-list">
                  <div className="advanced-toggle-row">
                    <label className="ssh-checkbox advanced-toggle-label">
                      <SelectionControl
                        checked={(form.reconnectMode ?? 'none') === 'none'}
                        name="serial-reconnect-mode"
                        type="radio"
                        onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'none' }))}
                      />
                      <span className="advanced-toggle-name">{t.reconnectNone}</span>
                    </label>
                    <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
                  </div>
                  <div className="advanced-toggle-row">
                    <label className="ssh-checkbox advanced-toggle-label">
                      <SelectionControl
                        checked={form.reconnectMode === 'enter'}
                        name="serial-reconnect-mode"
                        type="radio"
                        onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'enter' }))}
                      />
                      <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                    </label>
                    <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
                  </div>
                  <div className="advanced-toggle-row">
                    <label className="ssh-checkbox advanced-toggle-label">
                      <SelectionControl
                        checked={form.reconnectMode === 'auto'}
                        name="serial-reconnect-mode"
                        type="radio"
                        onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'auto' }))}
                      />
                      <span className="advanced-toggle-name">{t.autoReconnect}</span>
                    </label>
                    <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                  </div>
                </div>
                <label className="serial-reconnect-limit">
                  {t.serialReconnectMaxAttempts}:
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
                <p className="advanced-toggle-hint">{t.serialReconnectMaxAttemptsHint}</p>
              </div>
            </>
          ) : (
            <div className="terminal-key-box">
              <strong>{t.keySequence}</strong>
              <label>
                {t.backspaceKey}
                <DropdownSelect
                  value={form.backspaceKey ?? 'ASCII'}
                  options={[
                    { value: 'ASCII', label: 'ASCII - Backspace' },
                    { value: 'DEL', label: 'DEL - Backspace' }
                  ]}
                  onChange={(value) => setForm((prev) => ({ ...prev, backspaceKey: value }))}
                />
              </label>
              <label>
                {t.deleteKey}
                <DropdownSelect
                  value={form.deleteKey ?? 'VT220'}
                  options={[
                    { value: 'VT220', label: 'VT220 - Delete' },
                    { value: 'ASCII', label: 'ASCII - Delete' }
                  ]}
                  onChange={(value) => setForm((prev) => ({ ...prev, deleteKey: value }))}
                />
              </label>
              {form.type === 'ssh' ? (
                <>
                  <label>
                    {t.sshTerminalType}:
                    <DropdownSelect
                      value={form.terminalType ?? (isNetworkDevice ? 'vt100' : 'xterm-256color')}
                      options={[
                        { value: 'xterm-256color', label: 'xterm-256color' },
                        { value: 'xterm', label: 'xterm' },
                        { value: 'vt100', label: 'vt100' },
                        { value: 'vt220', label: 'vt220' },
                        { value: 'ansi', label: 'ansi' },
                        { value: 'linux', label: 'linux' }
                      ]}
                      onChange={(value) =>
                        setForm((prev) => ({
                          ...prev,
                          terminalType: value as CreateProfileInput['terminalType']
                        }))
                      }
                    />
                    <span className="ssh-field-hint ssh-terminal-type-hint">{t.sshTerminalTypeHint}</span>
                  </label>
                </>
              ) : null}
              {form.type === 'telnet' ? (
                <>
                  <label>
                    {t.telnetTerminalType}:
                    <DropdownSelect
                      value={form.terminalType ?? 'xterm-256color'}
                      options={[
                        { value: 'xterm-256color', label: 'xterm-256color' },
                        { value: 'vt100', label: 'vt100' },
                        { value: 'vt220', label: 'vt220' },
                        { value: 'ansi', label: 'ansi' }
                      ]}
                      onChange={(value) =>
                        setForm((prev) => ({
                          ...prev,
                          terminalType: value as CreateProfileInput['terminalType']
                        }))
                      }
                    />
                  </label>
                  <label>
                    {t.telnetNewline}:
                    <DropdownSelect
                      value={form.newlineMode ?? 'crlf'}
                      options={[
                        { value: 'none', label: t.telnetNewlineNone },
                        { value: 'lf', label: t.telnetNewlineLf },
                        { value: 'cr', label: t.telnetNewlineCr },
                        { value: 'crlf', label: t.telnetNewlineCrlf }
                      ]}
                      onChange={(value) =>
                        setForm((prev) => ({
                          ...prev,
                          newlineMode: value as CreateProfileInput['newlineMode']
                        }))
                      }
                    />
                  </label>
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.crNul !== false}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, crNul: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.telnetCrNul}</span>
                  </label>
                  <p className="advanced-toggle-hint">{t.telnetCrNulHint}</p>
                  <label className="full">
                    {t.telnetLoginScript}:
                    <textarea
                      rows={4}
                      spellCheck={false}
                      value={form.loginScript ?? ''}
                      onChange={(event) => setForm((prev) => ({ ...prev, loginScript: event.target.value }))}
                    />
                  </label>
                  <p className="ssh-field-hint full">{t.telnetLoginScriptHint}</p>
                  <p className="ssh-field-hint full">{t.telnetInsecureHint}</p>
                  <p className="ssh-field-hint full">{t.telnetTunnelHint}</p>
                </>
              ) : null}
            </div>
          )}
        </div>
      </fieldset>
    </div>
  )
}
