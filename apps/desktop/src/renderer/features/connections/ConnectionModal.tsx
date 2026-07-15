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
import { SshPrivateKeyField } from './SshPrivateKeyField'

export function ConnectionModal({
  errorMessage,
  groupOptions,
  mode,
  form,
  setForm,
  onClearHostFingerprint,
  onSubmit,
  onClose,
  standalone = false,
  profiles = []
}: {
  errorMessage: string | null
  groupOptions: string[]
  mode: ConnectionFormMode
  form: CreateProfileInput
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
          <CloseButton onClick={onClose} />
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
        <form className="ssh-form-shell" onSubmit={onSubmit}>
          {section === 'ssh' ? (
            <div className="ssh-form-page">
              <fieldset className="ssh-fieldset">
                <legend>{t.general}</legend>
                <div className="ssh-grid ssh-grid-general">
                  <label>
                    {t.connectionType}:
                    <span className="ft-select-shell">
                      <select
                        value={form.type}
                        onChange={(event) => {
                          const nextType = event.target.value as SessionType
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
                      >
                        <option value="ssh">SSH / SFTP</option>
                        <option value="ftp">FTP / FTPS</option>
                        <option value="telnet">Telnet</option>
                        <option value="serial">Serial</option>
                      </select>
                      <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                        expand_more
                      </span>
                    </span>
                  </label>
                  <label>
                    {t.group}:
                    <span className="ft-select-shell">
                      <select
                        value={form.group}
                        onChange={(event) => setForm((prev) => ({ ...prev, group: event.target.value }))}
                      >
                        {groupOptions.map((group) => (
                          <option key={group} value={group}>
                            {group}
                          </option>
                        ))}
                      </select>
                      <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                        expand_more
                      </span>
                    </span>
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
                        <select
                          value={form.dataBits ?? 8}
                          onChange={(event) =>
                            setForm((prev) => ({ ...prev, dataBits: Number(event.target.value) as 5 | 6 | 7 | 8 }))
                          }
                        >
                          <option value="5">5</option>
                          <option value="6">6</option>
                          <option value="7">7</option>
                          <option value="8">8</option>
                        </select>
                      </label>
                      <label>
                        Stop bits:
                        <select
                          value={form.stopBits ?? 1}
                          onChange={(event) =>
                            setForm((prev) => ({ ...prev, stopBits: Number(event.target.value) as 1 | 2 }))
                          }
                        >
                          <option value="1">1</option>
                          <option value="2">2</option>
                        </select>
                      </label>
                      <label>
                        Parity:
                        <select
                          value={form.parity ?? 'none'}
                          onChange={(event) =>
                            setForm((prev) => ({ ...prev, parity: event.target.value as CreateProfileInput['parity'] }))
                          }
                        >
                          <option value="none">None</option>
                          <option value="odd">Odd</option>
                          <option value="even">Even</option>
                          <option value="mark">Mark</option>
                          <option value="space">Space</option>
                        </select>
                      </label>
                      <label>
                        Flow control:
                        <select
                          value={form.flowControl ?? 'none'}
                          onChange={(event) =>
                            setForm((prev) => ({
                              ...prev,
                              flowControl: event.target.value as CreateProfileInput['flowControl']
                            }))
                          }
                        >
                          <option value="none">None</option>
                          <option value="hardware">Hardware</option>
                          <option value="software">Software</option>
                        </select>
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
                      <span className="ft-select-shell">
                        <select
                          value={form.authType ?? 'password'}
                          onChange={(event) =>
                            setForm((prev) => ({
                              ...prev,
                              authType: event.target.value as CreateProfileInput['authType']
                            }))
                          }
                        >
                          <option value="password">{t.password}</option>
                          <option value="privateKey">{t.privateKey}</option>
                          <option value="keyboard-interactive">Keyboard-interactive / MFA</option>
                          <option value="system">System / SSH agent</option>
                        </select>
                        <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                          expand_more
                        </span>
                      </span>
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
                    <div className="span-2 ssh-auth-hint">{t.passwordAuthHint}</div>
                  ) : form.type === 'ssh' && form.authType === 'keyboard-interactive' ? (
                    <div className="span-2 ssh-auth-hint">
                      可选：先尝试此密码；服务器需要 OTP/MFA 时，会弹出单独的验证码输入框。
                    </div>
                  ) : form.type === 'ftp' ? (
                    <>
                      <label className="span-2">
                        {t.ftpSecurityMode}:
                        <span className="ft-select-shell">
                          <select
                            value={form.securityMode ?? (form.secure ? 'explicit' : 'none')}
                            onChange={(event) => {
                              const securityMode = event.target.value as FtpSecurityMode
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
                          >
                            <option value="none">{t.ftpSecurityNone}</option>
                            <option value="explicit">{t.ftpSecurityExplicit}</option>
                            <option value="implicit">{t.ftpSecurityImplicit}</option>
                          </select>
                          <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                            expand_more
                          </span>
                        </span>
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
                  </div>
                  <div className="reconnect-mode-group">
                    <div className="reconnect-mode-group__label">断线行为</div>
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
                      <span className="material-symbols-outlined">account_tree</span>跳板机（ProxyJump）
                    </span>
                    <span className="jump-host-card__hint">先认证此 SSH 连接，再通过其安全通道访问目标主机。</span>
                    <span className="ft-select-shell">
                      <select
                        value={form.jumpProfileId ?? ''}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, jumpProfileId: event.target.value || undefined }))
                        }
                      >
                        <option value="">不使用跳板机（直连）</option>
                        {profiles
                          .filter((profile) => profile.type === 'ssh' && profile.id !== form.name)
                          .map((profile) => (
                            <option key={profile.id} value={profile.id}>
                              {profile.name} ({profile.host})
                            </option>
                          ))}
                      </select>
                      <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                        expand_more
                      </span>
                    </span>
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
                    <span className="ft-select-shell">
                      <select
                        value={form.encoding}
                        onChange={(event) => setForm((prev) => ({ ...prev, encoding: event.target.value }))}
                      >
                        <option value="UTF-8">UTF-8</option>
                        <option value="GBK">GBK</option>
                      </select>
                      <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                        expand_more
                      </span>
                    </span>
                  </label>
                  <div className="terminal-key-box">
                    <strong>{t.keySequence}</strong>
                    <label>
                      {t.backspaceKey}
                      <span className="ft-select-shell">
                        <select
                          value={form.backspaceKey}
                          onChange={(event) => setForm((prev) => ({ ...prev, backspaceKey: event.target.value }))}
                        >
                          <option value="ASCII">ASCII - Backspace</option>
                          <option value="DEL">DEL - Backspace</option>
                        </select>
                        <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                          expand_more
                        </span>
                      </span>
                    </label>
                    <label>
                      {t.deleteKey}
                      <span className="ft-select-shell">
                        <select
                          value={form.deleteKey}
                          onChange={(event) => setForm((prev) => ({ ...prev, deleteKey: event.target.value }))}
                        >
                          <option value="VT220">VT220 - Delete</option>
                          <option value="ASCII">ASCII - Delete</option>
                        </select>
                        <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                          expand_more
                        </span>
                      </span>
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
                    <span className="ft-select-shell">
                      <select
                        value={form.proxy?.type ?? 'none'}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            proxy: {
                              ...(prev.proxy ?? { host: '', port: 1080 }),
                              type: event.target.value as 'none' | 'socks5' | 'http'
                            }
                          }))
                        }
                      >
                        <option value="none">Direct</option>
                        <option value="socks5">SOCKS5</option>
                        <option value="http">HTTP CONNECT</option>
                      </select>
                      <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                        expand_more
                      </span>
                    </span>
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
                  <p>隧道在 SSH 连接成功后启动；断线或关闭标签时会自动回收。</p>
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
                        setForm((prev) => ({ ...prev, forwards: prev.forwards?.filter((item) => item.id !== rule.id) }))
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
                  <span className="material-symbols-outlined">add</span>添加隧道
                </button>
              </fieldset>
            </div>
          ) : null}
          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
          <div className="form-actions ssh-actions">
            <button className="flat-button" onClick={onClose} type="button">
              {t.cancel}
            </button>
            <button className="primary-button" type="submit">
              {mode === 'edit' ? t.saveChanges : t.saveConnection}
            </button>
          </div>
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
          <strong>{rule.kind === 'local' ? '本地转发' : rule.kind === 'remote' ? '远程转发' : '动态 SOCKS5'}</strong>
        </div>
        <button
          type="button"
          className="tunnel-remove-button"
          aria-label="删除隧道"
          title="删除隧道"
          onClick={onRemove}
        >
          <span className="material-symbols-outlined">delete</span>
        </button>
      </header>
      <div className="tunnel-rule-grid">
        <label>
          类型
          <span className="ft-select-shell">
            <select
              value={rule.kind}
              onChange={(event) =>
                onChange({
                  kind: event.target.value as SshForwardRule['kind'],
                  ...(event.target.value === 'dynamic' ? { targetHost: undefined, targetPort: undefined } : {})
                })
              }
            >
              <option value="local">本地 (-L)</option>
              <option value="remote">远程 (-R)</option>
              <option value="dynamic">动态 (-D)</option>
            </select>
            <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
              expand_more
            </span>
          </span>
        </label>
        <label>
          监听地址
          <input value={rule.bindHost} onChange={(event) => onChange({ bindHost: event.target.value })} />
        </label>
        <label>
          监听端口
          <input
            inputMode="numeric"
            value={rule.bindPort || ''}
            onChange={(event) => onChange({ bindPort: Number(event.target.value) })}
          />
        </label>
        {!isDynamic ? (
          <>
            <label>
              目标主机
              <input value={rule.targetHost ?? ''} onChange={(event) => onChange({ targetHost: event.target.value })} />
            </label>
            <label>
              目标端口
              <input
                inputMode="numeric"
                value={rule.targetPort || ''}
                onChange={(event) => onChange({ targetPort: Number(event.target.value) })}
              />
            </label>
          </>
        ) : (
          <div className="tunnel-socks-note">
            <span className="material-symbols-outlined">vpn_key</span>客户端连接此地址后自行指定目标。
          </div>
        )}
      </div>
      <label className="tunnel-autostart ssh-checkbox">
        <input
          type="checkbox"
          checked={rule.autoStart}
          onChange={(event) => onChange({ autoStart: event.target.checked })}
        />
        连接后自动启动
      </label>
    </article>
  )
}
