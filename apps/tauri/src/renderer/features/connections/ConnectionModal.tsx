import { useState, type FormEvent } from 'react'
import type {
  ConnectionFormMode,
  CreateProfileInput,
  FtpSecurityMode,
  SessionType,
  SshForwardRule
} from '@fileterm/core'
import { normalizeConnectionHost } from '@fileterm/shared'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'
import { SshPrivateKeyField } from './SshPrivateKeyField'

export function ConnectionModal({
  errorMessage,
  groupOptions,
  isSubmitting = false,
  mode,
  form,
  hasSavedPassword = false,
  setForm,
  onClearHostFingerprint,
  onSubmit,
  onClose,
  standalone = false,
  profiles = []
}: {
  errorMessage: string | null
  groupOptions: string[]
  isSubmitting?: boolean
  mode: ConnectionFormMode
  form: CreateProfileInput
  hasSavedPassword?: boolean
  setForm(value: CreateProfileInput | ((prev: CreateProfileInput) => CreateProfileInput)): void
  onClearHostFingerprint?(): void
  onSubmit(event: FormEvent<HTMLFormElement>): void
  onClose(): void
  standalone?: boolean
  profiles?: import('@fileterm/core').ConnectionProfile[]
}) {
  const [section, setSection] = useState<'ssh' | 'terminal' | 'proxy' | 'tunnel'>('ssh')
  const supportsProxy = form.type === 'ssh' || form.type === 'telnet'

  const content = (
    <div className={`modal-card ssh-modal ${standalone ? 'standalone' : ''}`}>
      <div className="connection-manager-header" data-tauri-drag-region={standalone ? 'deep' : undefined}>
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">settings_ethernet</span>
          <span>{mode === 'edit' ? t.editConnection : t.newConnection}</span>
        </span>
        <div className="connection-manager-header-actions">
          <CloseButton disabled={isSubmitting} onClick={onClose} />
        </div>
      </div>
      <div className="ssh-modal-body">
        <aside className="ssh-modal-nav">
          <button className={section === 'ssh' ? 'active' : ''} type="button" onClick={() => setSection('ssh')}>
            {t.sshConnection}
          </button>
          <button
            className={section === 'terminal' ? 'active' : ''}
            type="button"
            onClick={() => setSection('terminal')}
          >
            {t.terminal}
          </button>
          {supportsProxy ? (
            <button className={section === 'proxy' ? 'active' : ''} type="button" onClick={() => setSection('proxy')}>
              {t.proxyServer}
            </button>
          ) : null}
          {form.type === 'ssh' ? (
            <button className={section === 'tunnel' ? 'active' : ''} type="button" onClick={() => setSection('tunnel')}>
              {t.tunnel}
            </button>
          ) : null}
        </aside>
        <form aria-busy={isSubmitting} className="ssh-form-shell" onSubmit={onSubmit}>
          <fieldset
            className="connection-form-submit-lock"
            disabled={isSubmitting}
            style={{ border: 0, display: 'contents', margin: 0, padding: 0 }}
          >
            {section === 'ssh' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.general}</legend>
                  <div className="ssh-grid ssh-grid-general">
                    <label>
                      {t.connectionType}:
                      <DropdownSelect
                        value={form.type}
                        options={[
                          { value: 'ssh', label: 'SSH / SFTP' },
                          { value: 'ftp', label: 'FTP / FTPS' },
                          { value: 'telnet', label: 'Telnet' },
                          { value: 'serial', label: 'Serial' }
                        ]}
                        onChange={(value) => {
                          const nextType = value as SessionType
                          const defaults: Record<SessionType, number> = { ssh: 22, ftp: 21, telnet: 23, serial: 0 }
                          setForm((prev) => ({
                            ...prev,
                            type: nextType,
                            port:
                              prev.port === 22 || prev.port === 21 || prev.port === 23 || !prev.port
                                ? defaults[nextType]
                                : prev.port,
                            authType: nextType === 'ssh' ? (prev.authType ?? 'system') : 'password',
                            remotePath: nextType === 'ssh' || nextType === 'ftp' ? prev.remotePath || '/' : ''
                          }))
                        }}
                      />
                    </label>
                    <label>
                      {t.group}:
                      <DropdownSelect
                        value={form.group ?? ''}
                        options={groupOptions.map((group) => ({ value: group, label: group }))}
                        onChange={(value) => setForm((prev) => ({ ...prev, group: value }))}
                      />
                    </label>
                    <label className="span-2">
                      {t.name}:
                      <input
                        value={form.name}
                        onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))}
                      />
                    </label>
                    {form.type === 'serial' ? (
                      <label className="span-2">
                        Device path:
                        <input
                          placeholder="COM3 / /dev/ttyUSB0 / /dev/cu.usbserial"
                          spellCheck={false}
                          value={form.devicePath ?? ''}
                          onChange={(event) => setForm((prev) => ({ ...prev, devicePath: event.target.value }))}
                        />
                      </label>
                    ) : (
                      <label className="span-2">
                        {t.host}:
                        <input
                          placeholder="example.com / 192.168.1.10 / 2001:db8::10"
                          spellCheck={false}
                          value={form.host}
                          onBlur={(event) => {
                            const normalizedHost = normalizeConnectionHost(event.target.value)
                            if (normalizedHost !== event.target.value) {
                              setForm((prev) => ({ ...prev, host: normalizedHost }))
                            }
                          }}
                          onChange={(event) => setForm((prev) => ({ ...prev, host: event.target.value }))}
                        />
                      </label>
                    )}
                    {form.type !== 'serial' ? <div className="span-2 ssh-field-hint">{t.hostInputHint}</div> : null}
                    {form.type !== 'serial' ? (
                      <label className="narrow">
                        {t.port}:
                        <input
                          inputMode="numeric"
                          value={form.port || ''}
                          onChange={(event) =>
                            setForm((prev) => ({ ...prev, port: Number(event.target.value.replace(/\D/g, '')) }))
                          }
                        />
                      </label>
                    ) : null}
                    {form.type === 'ssh' || form.type === 'ftp' ? (
                      <label>
                        {t.remotePath}:
                        <input
                          value={form.remotePath}
                          onChange={(event) => setForm((prev) => ({ ...prev, remotePath: event.target.value }))}
                        />
                      </label>
                    ) : null}
                    {form.type === 'serial' ? (
                      <div className="span-2 ssh-grid">
                        <label>
                          Baud rate:
                          <input
                            inputMode="numeric"
                            value={form.baudRate ?? 115200}
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, baudRate: Number(event.target.value) || 115200 }))
                            }
                          />
                        </label>
                        <label>
                          Data bits:
                          <DropdownSelect
                            value={String(form.dataBits ?? 8)}
                            options={[
                              { value: '5', label: '5' },
                              { value: '6', label: '6' },
                              { value: '7', label: '7' },
                              { value: '8', label: '8' }
                            ]}
                            onChange={(value) =>
                              setForm((prev) => ({ ...prev, dataBits: Number(value) as 5 | 6 | 7 | 8 }))
                            }
                          />
                        </label>
                        <label>
                          Stop bits:
                          <DropdownSelect
                            value={String(form.stopBits ?? 1)}
                            options={[
                              { value: '1', label: '1' },
                              { value: '2', label: '2' }
                            ]}
                            onChange={(value) => setForm((prev) => ({ ...prev, stopBits: Number(value) as 1 | 2 }))}
                          />
                        </label>
                        <label>
                          Parity:
                          <DropdownSelect
                            value={form.parity ?? 'none'}
                            options={[
                              { value: 'none', label: 'None' },
                              { value: 'odd', label: 'Odd' },
                              { value: 'even', label: 'Even' },
                              { value: 'mark', label: 'Mark' },
                              { value: 'space', label: 'Space' }
                            ]}
                            onChange={(value) =>
                              setForm((prev) => ({ ...prev, parity: value as CreateProfileInput['parity'] }))
                            }
                          />
                        </label>
                        <label>
                          Flow control:
                          <DropdownSelect
                            value={form.flowControl ?? 'none'}
                            options={[
                              { value: 'none', label: 'None' },
                              { value: 'hardware', label: 'Hardware' },
                              { value: 'software', label: 'Software' }
                            ]}
                            onChange={(value) =>
                              setForm((prev) => ({
                                ...prev,
                                flowControl: value as CreateProfileInput['flowControl']
                              }))
                            }
                          />
                        </label>
                      </div>
                    ) : null}
                    <label className="full">
                      {t.note}:
                      <textarea
                        value={form.note ?? ''}
                        onChange={(event) => setForm((prev) => ({ ...prev, note: event.target.value }))}
                      />
                    </label>
                  </div>
                </fieldset>
                <fieldset className="ssh-fieldset">
                  <legend>{t.auth}</legend>
                  <div className="ssh-grid ssh-grid-auth">
                    {form.type === 'ssh' ? (
                      <label>
                        {t.method}:
                        <DropdownSelect
                          value={form.authType ?? 'password'}
                          options={[
                            { value: 'password', label: t.password },
                            { value: 'privateKey', label: t.privateKey },
                            { value: 'keyboard-interactive', label: 'Keyboard-interactive / MFA' },
                            { value: 'system', label: 'System / SSH agent' }
                          ]}
                          onChange={(value) =>
                            setForm((prev) => ({ ...prev, authType: value as CreateProfileInput['authType'] }))
                          }
                        />
                      </label>
                    ) : null}
                    {form.type !== 'telnet' && form.type !== 'serial' ? (
                      <label>
                        {t.username}:
                        <input
                          value={form.username}
                          onChange={(event) => setForm((prev) => ({ ...prev, username: event.target.value }))}
                        />
                      </label>
                    ) : null}
                    {form.type === 'ftp' || form.authType === 'password' || form.authType === 'keyboard-interactive' ? (
                      <label className="span-2">
                        {t.password}:
                        <input
                          type="password"
                          value={form.password ?? ''}
                          onChange={(event) => setForm((prev) => ({ ...prev, password: event.target.value }))}
                        />
                      </label>
                    ) : null}
                    {form.type === 'ssh' && form.authType === 'privateKey' ? (
                      <SshPrivateKeyField form={form} setForm={setForm} />
                    ) : null}
                    {form.type === 'ssh' && form.authType === 'password' ? (
                      <div className="span-2 ssh-auth-hint">
                        {mode === 'edit' && hasSavedPassword ? t.passwordSavedHint : t.passwordAuthHint}
                      </div>
                    ) : form.type === 'ssh' && form.authType === 'keyboard-interactive' ? (
                      <div className="span-2 ssh-auth-hint">{t.keyboardInteractiveHint}</div>
                    ) : form.type === 'ftp' ? (
                      <>
                        <label className="span-2">
                          {t.ftpSecurityMode}:
                          <DropdownSelect
                            value={form.securityMode ?? (form.secure ? 'explicit' : 'none')}
                            options={[
                              { value: 'none', label: t.ftpSecurityNone },
                              { value: 'explicit', label: t.ftpSecurityExplicit },
                              { value: 'implicit', label: t.ftpSecurityImplicit }
                            ]}
                            onChange={(value) => {
                              const securityMode = value as FtpSecurityMode
                              setForm((prev) => ({
                                ...prev,
                                securityMode,
                                secure: securityMode !== 'none',
                                port:
                                  securityMode === 'implicit' && prev.port === 21
                                    ? 990
                                    : securityMode !== 'implicit' && prev.port === 990
                                      ? 21
                                      : prev.port
                              }))
                            }}
                          />
                        </label>
                        <div className="span-2 ssh-auth-hint">{t.ftpAuthHint}</div>
                      </>
                    ) : null}
                    {form.type === 'ssh' && mode === 'edit' && form.trustedHostFingerprint ? (
                      <div className="span-2 saved-fingerprint-card">
                        <span aria-hidden="true" className="material-symbols-outlined saved-fingerprint-card__icon">
                          fingerprint
                        </span>
                        <div className="saved-fingerprint-card__content">
                          <strong>{t.savedHostFingerprint}</strong>
                          <p>{t.clearSavedFingerprintHint}</p>
                        </div>
                        <button
                          className="flat-button compact saved-fingerprint-card__action"
                          onClick={onClearHostFingerprint}
                          type="button"
                        >
                          <span aria-hidden="true" className="material-symbols-outlined">
                            restart_alt
                          </span>
                          {t.clearSavedFingerprint}
                        </button>
                      </div>
                    ) : null}
                  </div>
                </fieldset>
                {form.type === 'ssh' ? (
                  <fieldset className="ssh-fieldset">
                    <legend>{t.advanced}</legend>
                    <div className="advanced-toggle-list">
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={Boolean(form.enableExecChannel)}
                            type="checkbox"
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, enableExecChannel: event.target.checked }))
                            }
                          />
                          <span className="advanced-toggle-name">{t.enableExecChannel}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.enableExecChannelHint}</p>
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={form.enableResourceMonitoring !== false}
                            type="checkbox"
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, enableResourceMonitoring: event.target.checked }))
                            }
                          />
                          <span className="advanced-toggle-name">{t.resourceMonitoring}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.resourceMonitoringDescription}</p>
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={Boolean(form.legacyAlgorithms)}
                            type="checkbox"
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, legacyAlgorithms: event.target.checked }))
                            }
                          />
                          <span className="advanced-toggle-name">{t.legacyAlgorithms}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.legacyAlgorithmsHint}</p>
                      </div>
                    </div>
                    <div className="reconnect-mode-group">
                      <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
                      <div className="advanced-toggle-list">
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={!form.reconnectMode || form.reconnectMode === 'none'}
                              type="checkbox"
                              onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'none' }))}
                            />
                            <span className="advanced-toggle-name">{t.reconnectNone}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
                        </div>
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={form.reconnectMode === 'enter'}
                              type="checkbox"
                              onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'enter' }))}
                            />
                            <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
                        </div>
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={form.reconnectMode === 'auto'}
                              type="checkbox"
                              onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'auto' }))}
                            />
                            <span className="advanced-toggle-name">{t.autoReconnect}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                        </div>
                      </div>
                    </div>
                    <label className="jump-host-card">
                      <span className="jump-host-card__title">
                        <span className="material-symbols-outlined">account_tree</span>
                        {t.proxyJump}
                      </span>
                      <span className="jump-host-card__hint">{t.proxyJumpHint}</span>
                      <DropdownSelect
                        value={form.jumpProfileId ?? ''}
                        options={[
                          { value: '', label: t.noProxyJump },
                          ...profiles
                            .filter((profile) => profile.type === 'ssh' && profile.id !== form.name)
                            .map((profile) => ({
                              value: profile.id,
                              label: `${profile.name} (${profile.host})`
                            }))
                        ]}
                        onChange={(value) => setForm((prev) => ({ ...prev, jumpProfileId: value || undefined }))}
                      />
                    </label>
                  </fieldset>
                ) : null}
              </div>
            ) : null}
            {section === 'terminal' ? (
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
                    </div>
                  </div>
                </fieldset>
              </div>
            ) : null}
            {section === 'proxy' && supportsProxy ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.proxyServer}</legend>
                  <div className="ssh-grid">
                    <label>
                      Type:
                      <DropdownSelect
                        value={form.proxy?.type ?? 'none'}
                        options={[
                          { value: 'none', label: 'Direct' },
                          { value: 'socks5', label: 'SOCKS5' },
                          { value: 'http', label: 'HTTP CONNECT' }
                        ]}
                        onChange={(value) =>
                          setForm((prev) => ({
                            ...prev,
                            proxy: {
                              ...(prev.proxy ?? { host: '', port: 1080 }),
                              type: value as 'none' | 'socks5' | 'http'
                            }
                          }))
                        }
                      />
                    </label>
                    {form.proxy?.type && form.proxy.type !== 'none' ? (
                      <>
                        <label>
                          Host:
                          <input
                            value={form.proxy.host}
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, proxy: { ...prev.proxy!, host: event.target.value } }))
                            }
                          />
                        </label>
                        <label>
                          Port:
                          <input
                            inputMode="numeric"
                            value={form.proxy.port}
                            onChange={(event) =>
                              setForm((prev) => ({
                                ...prev,
                                proxy: { ...prev.proxy!, port: Number(event.target.value) }
                              }))
                            }
                          />
                        </label>
                        <label>
                          Username:
                          <input
                            value={form.proxy.username ?? ''}
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, proxy: { ...prev.proxy!, username: event.target.value } }))
                            }
                          />
                        </label>
                        <label>
                          Password:
                          <input
                            type="password"
                            value={form.proxyPassword ?? ''}
                            onChange={(event) => setForm((prev) => ({ ...prev, proxyPassword: event.target.value }))}
                          />
                        </label>
                      </>
                    ) : null}
                  </div>
                </fieldset>
              </div>
            ) : null}
            {section === 'tunnel' && form.type === 'ssh' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset tunnel-fieldset">
                  <legend>{t.tunnel}</legend>
                  <div className="tunnel-intro">
                    <span className="material-symbols-outlined">lan</span>
                    <p>{t.tunnelAutoStartHint}</p>
                  </div>
                  <div className="tunnel-rule-list">
                    {(form.forwards ?? []).map((rule, index) => (
                      <TunnelRuleEditor
                        key={rule.id}
                        index={index}
                        rule={rule}
                        onChange={(patch) =>
                          setForm((prev) => ({
                            ...prev,
                            forwards: prev.forwards?.map((item) => (item.id === rule.id ? { ...item, ...patch } : item))
                          }))
                        }
                        onRemove={() =>
                          setForm((prev) => ({
                            ...prev,
                            forwards: prev.forwards?.filter((item) => item.id !== rule.id)
                          }))
                        }
                      />
                    ))}
                  </div>
                  <button
                    type="button"
                    className="tunnel-add-button"
                    onClick={() =>
                      setForm((prev) => ({
                        ...prev,
                        forwards: [
                          ...(prev.forwards ?? []),
                          {
                            id: crypto.randomUUID(),
                            kind: 'local',
                            bindHost: '127.0.0.1',
                            bindPort: 0,
                            targetHost: '127.0.0.1',
                            targetPort: 0,
                            autoStart: true
                          }
                        ]
                      }))
                    }
                  >
                    <span className="material-symbols-outlined">add</span>
                    {t.addConnectionTunnel}
                  </button>
                </fieldset>
              </div>
            ) : null}
            {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
            <div className="form-actions ssh-actions">
              <button className="flat-button" disabled={isSubmitting} onClick={onClose} type="button">
                {t.cancel}
              </button>
              <button className="primary-button" disabled={isSubmitting} type="submit">
                {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
                <span>{mode === 'edit' ? t.saveChanges : t.saveConnection}</span>
              </button>
            </div>
          </fieldset>
        </form>
      </div>
    </div>
  )

  if (standalone) {
    return <div className="connection-form-window">{content}</div>
  }

  return <div className="modal-backdrop">{content}</div>
}

function TunnelRuleEditor({
  rule,
  index,
  onChange,
  onRemove
}: {
  rule: SshForwardRule
  index: number
  onChange(patch: Partial<SshForwardRule>): void
  onRemove(): void
}) {
  const isDynamic = rule.kind === 'dynamic'
  return (
    <article className="tunnel-rule-card">
      <header>
        <div>
          <span className="tunnel-rule-index">{String(index + 1).padStart(2, '0')}</span>
          <strong>
            {rule.kind === 'local' ? t.localForward : rule.kind === 'remote' ? t.remoteForward : t.dynamicSocks5}
          </strong>
        </div>
        <button
          type="button"
          className="tunnel-remove-button"
          aria-label={t.deleteTunnel}
          title={t.deleteTunnel}
          onClick={onRemove}
        >
          <span className="material-symbols-outlined">delete</span>
        </button>
      </header>
      <div className="tunnel-rule-grid">
        <label>
          {t.tunnelType}
          <DropdownSelect
            value={rule.kind}
            options={[
              { value: 'local', label: t.localForwardShort },
              { value: 'remote', label: t.remoteForwardShort },
              { value: 'dynamic', label: t.dynamicForwardShort }
            ]}
            onChange={(value) =>
              onChange({
                kind: value as SshForwardRule['kind'],
                ...(value === 'dynamic' ? { targetHost: undefined, targetPort: undefined } : {})
              })
            }
          />
        </label>
        <label>
          {t.tunnelBindHost}
          <input value={rule.bindHost} onChange={(event) => onChange({ bindHost: event.target.value })} />
        </label>
        <label>
          {t.tunnelBindPort}
          <input
            inputMode="numeric"
            value={rule.bindPort || ''}
            onChange={(event) => onChange({ bindPort: Number(event.target.value) })}
          />
        </label>
        {!isDynamic ? (
          <>
            <label>
              {t.tunnelTargetHost}
              <input value={rule.targetHost ?? ''} onChange={(event) => onChange({ targetHost: event.target.value })} />
            </label>
            <label>
              {t.tunnelTargetPort}
              <input
                inputMode="numeric"
                value={rule.targetPort || ''}
                onChange={(event) => onChange({ targetPort: Number(event.target.value) })}
              />
            </label>
          </>
        ) : (
          <div className="tunnel-socks-note">
            <span className="material-symbols-outlined">vpn_key</span>
            {t.tunnelClientTargetHint}
          </div>
        )}
      </div>
      <label className="tunnel-autostart ssh-checkbox">
        <input
          type="checkbox"
          checked={rule.autoStart}
          onChange={(event) => onChange({ autoStart: event.target.checked })}
        />
        {t.autoStartAfterConnect}
      </label>
    </article>
  )
}
