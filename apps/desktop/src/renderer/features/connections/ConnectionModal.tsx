import { useState, type FormEvent } from 'react'
import type { ConnectionFormMode, CreateProfileInput } from '@termdock/core'
import { t } from '../../i18n'

export function ConnectionModal({
  errorMessage,
  groupOptions,
  mode,
  form,
  setForm,
  onClearHostFingerprint,
  onSubmit,
  onClose,
  standalone = false
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
}) {
  const [section, setSection] = useState<'ssh' | 'terminal' | 'proxy' | 'tunnel'>('ssh')

  const content = (
    <div className={`modal-card ssh-modal ${standalone ? 'standalone' : ''}`}>
      <div className="modal-header">
        <span>{mode === 'edit' ? t.editConnection : t.newConnection}</span>
        {!standalone ? <button className="icon-button" onClick={onClose} type="button">×</button> : null}
      </div>
      <div className="ssh-modal-body">
        <aside className="ssh-modal-nav">
          <button className={section === 'ssh' ? 'active' : ''} type="button" onClick={() => setSection('ssh')}>{t.sshConnection}</button>
          <button className={section === 'terminal' ? 'active' : ''} type="button" onClick={() => setSection('terminal')}>{t.terminal}</button>
          <button className={section === 'proxy' ? 'active' : ''} type="button" onClick={() => setSection('proxy')}>{t.proxyServer}</button>
          <button className={section === 'tunnel' ? 'active' : ''} type="button" onClick={() => setSection('tunnel')}>{t.tunnel}</button>
        </aside>
        <form className="ssh-form-shell" onSubmit={onSubmit}>
          {section === 'ssh' ? (
            <div className="ssh-form-page">
              <fieldset className="ssh-fieldset">
                <legend>{t.general}</legend>
                <div className="ssh-grid ssh-grid-general">
                  <label>{t.connectionType}:
                    <select
                      value={form.type}
                      onChange={(event) => {
                        const nextType = event.target.value as 'ssh' | 'ftp'
                        setForm((prev) => ({
                          ...prev,
                          type: nextType,
                          port: nextType === 'ftp' && prev.port === 22 ? 21 : nextType === 'ssh' && prev.port === 21 ? 22 : prev.port,
                          authType: nextType === 'ssh' ? prev.authType ?? 'system' : 'password'
                        }))
                      }}
                    >
                      <option value="ssh">SSH / SFTP</option>
                      <option value="ftp">FTP / FTPS</option>
                    </select>
                  </label>
                  <label>{t.group}:
                    <select value={form.group} onChange={(event) => setForm((prev) => ({ ...prev, group: event.target.value }))}>
                      {groupOptions.map((group) => (
                        <option key={group} value={group}>{group}</option>
                      ))}
                    </select>
                  </label>
                  <label className="span-2">{t.name}:<input value={form.name} onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))} /></label>
                  <label className="span-2">{t.host}:<input value={form.host} onChange={(event) => setForm((prev) => ({ ...prev, host: event.target.value }))} /></label>
                  <label className="narrow">{t.port}:<input inputMode="numeric" value={form.port || ''} onChange={(event) => setForm((prev) => ({ ...prev, port: Number(event.target.value.replace(/\D/g, '')) }))} /></label>
                  <label>{t.remotePath}:<input value={form.remotePath} onChange={(event) => setForm((prev) => ({ ...prev, remotePath: event.target.value }))} /></label>
                  <label className="full">{t.note}:<textarea value={form.note ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, note: event.target.value }))} /></label>
                </div>
              </fieldset>
              <fieldset className="ssh-fieldset">
                <legend>{t.auth}</legend>
                <div className="ssh-grid ssh-grid-auth">
                  {form.type === 'ssh' ? (
                    <label>{t.method}:
                      <select value={form.authType === 'system' ? 'password' : form.authType} onChange={(event) => setForm((prev) => ({ ...prev, authType: event.target.value as 'password' | 'privateKey' }))}>
                        <option value="password">{t.password}</option>
                        <option value="privateKey">{t.privateKey}</option>
                      </select>
                    </label>
                  ) : null}
                  <label>{t.username}:<input value={form.username} onChange={(event) => setForm((prev) => ({ ...prev, username: event.target.value }))} /></label>
                  {(form.type === 'ftp' || form.authType === 'password') ? (
                    <label className="span-2">{t.password}:<input type="password" value={form.password ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, password: event.target.value }))} /></label>
                  ) : null}
                  {form.type === 'ssh' && form.authType === 'privateKey' ? (
                    <label className="full">{t.privateKey}:<input value={form.privateKeyPath ?? ''} onChange={(event) => setForm((prev) => ({ ...prev, privateKeyPath: event.target.value }))} /></label>
                  ) : null}
                  {form.type === 'ssh' && form.authType === 'password' ? (
                    <div className="span-2 ssh-auth-hint">{t.passwordAuthHint}</div>
                  ) : (
                    form.type === 'ftp' ? (
                      <>
                        <label className="ssh-checkbox span-2">
                          <input checked={Boolean(form.secure)} type="checkbox" onChange={(event) => setForm((prev) => ({ ...prev, secure: event.target.checked }))} />
                          <span>{t.useFtps}</span>
                        </label>
                        <div className="span-2 ssh-auth-hint">{t.ftpAuthHint}</div>
                      </>
                    ) : null
                  )}
                  {form.type === 'ssh' && mode === 'edit' && form.trustedHostFingerprint ? (
                    <div className="span-2 ssh-inline-action">
                      <button className="flat-button compact" onClick={onClearHostFingerprint} type="button">
                        {t.clearSavedFingerprint}
                      </button>
                    </div>
                  ) : null}
                </div>
              </fieldset>
              {form.type === 'ssh' ? <fieldset className="ssh-fieldset">
                <legend>{t.advanced}</legend>
                <label className="ssh-checkbox">
                  <input checked={Boolean(form.enableExecChannel)} type="checkbox" onChange={(event) => setForm((prev) => ({ ...prev, enableExecChannel: event.target.checked }))} />
                  <span>{t.enableExecChannel}</span>
                </label>
                <div className="ssh-field-hint">{t.enableExecChannelHint}</div>
              </fieldset> : null}
            </div>
          ) : null}
          {section === 'terminal' ? (
            <div className="ssh-form-page">
              <fieldset className="ssh-fieldset narrow">
                <legend>{t.terminal}</legend>
                <div className="ssh-grid single">
                  <label>{t.characterEncoding}:
                    <select value={form.encoding} onChange={(event) => setForm((prev) => ({ ...prev, encoding: event.target.value }))}>
                      <option value="UTF-8">UTF-8</option>
                      <option value="GBK">GBK</option>
                    </select>
                  </label>
                  <div className="terminal-key-box">
                    <strong>{t.keySequence}</strong>
                    <label>{t.backspaceKey}
                      <select value={form.backspaceKey} onChange={(event) => setForm((prev) => ({ ...prev, backspaceKey: event.target.value }))}>
                        <option value="ASCII">ASCII - Backspace</option>
                        <option value="DEL">DEL - Backspace</option>
                      </select>
                    </label>
                    <label>{t.deleteKey}
                      <select value={form.deleteKey} onChange={(event) => setForm((prev) => ({ ...prev, deleteKey: event.target.value }))}>
                        <option value="VT220">VT220 - Delete</option>
                        <option value="ASCII">ASCII - Delete</option>
                      </select>
                    </label>
                  </div>
                </div>
              </fieldset>
            </div>
          ) : null}
          {section === 'proxy' ? <div className="ssh-placeholder">{t.proxyComingSoon}</div> : null}
          {section === 'tunnel' ? <div className="ssh-placeholder">{t.tunnelComingSoon}</div> : null}
          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
          <div className="form-actions ssh-actions">
            <button className="flat-button" onClick={onClose} type="button">{t.cancel}</button>
            <button className="primary-button" type="submit">{mode === 'edit' ? t.saveChanges : t.saveConnection}</button>
          </div>
        </form>
      </div>
    </div>
  )

  if (standalone) {
    return <div className="connection-form-window">{content}</div>
  }

  return (
    <div className="modal-backdrop">
      {content}
    </div>
  )
}
