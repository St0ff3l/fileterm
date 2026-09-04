import type { ConnectionFormMode, CreateProfileInput, FtpSecurityMode, SshConnectionDefaults } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { ConnectionSecretField } from './connection-secret-field'
import { SshPrivateKeyField } from './ssh-private-key-field'
import { effectiveConnectionSetting, type ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionSshAuthSection({
  connectionDefaults,
  form,
  hasSavedPassword,
  hasSavedSuPassword,
  hasSavedSudoPassword,
  isNetworkDevice,
  mode,
  onClearHostFingerprint,
  setForm
}: {
  connectionDefaults: SshConnectionDefaults
  form: CreateProfileInput
  hasSavedPassword: boolean
  hasSavedSuPassword: boolean
  hasSavedSudoPassword: boolean
  isNetworkDevice: boolean
  mode: ConnectionFormMode
  onClearHostFingerprint?(): void
  setForm: ConnectionFormSetter
}) {
  const isKeyboardInteractiveAuth = form.authType === 'keyboard-interactive' || form.authType === 'jumpserver-koko-mfa'

  return (
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
                { value: 'keyboard-interactive', label: t.keyboardInteractiveAuth },
                { value: 'jumpserver-koko-mfa', label: t.jumpServerKokoMfaAuth },
                { value: 'system', label: t.systemSshAuth }
              ]}
              onChange={(value) => setForm((prev) => ({ ...prev, authType: value as CreateProfileInput['authType'] }))}
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
        {form.type === 'ftp' || form.authType === 'password' || isKeyboardInteractiveAuth ? (
          <ConnectionSecretField
            id="connection-password"
            label={t.password}
            value={form.password}
            hasSavedValue={hasSavedPassword}
            canClear={mode === 'edit'}
            disabled={
              form.type === 'ssh' &&
              form.authType === 'password' &&
              effectiveConnectionSetting(form, connectionDefaults, 'useEmptyPassword')
            }
            onChange={(value) =>
              setForm((prev) => ({
                ...prev,
                password: value,
                useEmptyPassword: value ? false : prev.useEmptyPassword
              }))
            }
            onClear={() => setForm((prev) => ({ ...prev, password: null, useEmptyPassword: false }))}
            onUndo={() => setForm((prev) => ({ ...prev, password: '' }))}
          />
        ) : null}
        {form.type === 'ssh' && form.authType === 'privateKey' ? (
          <SshPrivateKeyField form={form} setForm={setForm} />
        ) : null}
        {form.type === 'ssh' && isKeyboardInteractiveAuth ? (
          <div className="span-2 ssh-auth-hint">
            <div>{t.keyboardInteractiveHint}</div>
            {form.authType === 'jumpserver-koko-mfa' ? <div>{t.jumpServerInteractiveGatewayHint}</div> : null}
          </div>
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
                    certificateFingerprint: securityMode === 'none' ? '' : prev.certificateFingerprint,
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
            {form.securityMode !== 'none' ? (
              <label className="span-2">
                {t.ftpCertificateFingerprint}:
                <input
                  value={form.certificateFingerprint ?? ''}
                  placeholder="sha256:..."
                  onChange={(event) => setForm((prev) => ({ ...prev, certificateFingerprint: event.target.value }))}
                />
                <span className="ssh-field-hint">{t.ftpCertificateFingerprintHint}</span>
              </label>
            ) : null}
            <label className="span-2">
              {t.ftpTransferMode}:
              <DropdownSelect
                value={form.transferMode ?? 'passive'}
                options={[
                  { value: 'passive', label: t.ftpTransferPassive },
                  { value: 'active', label: t.ftpTransferActive }
                ]}
                onChange={(value) =>
                  setForm((prev) => ({
                    ...prev,
                    transferMode: value as CreateProfileInput['transferMode']
                  }))
                }
              />
              <span className="ssh-field-hint">{t.ftpTransferModeHint}</span>
            </label>
            <div className="span-2 ssh-auth-hint">{t.ftpAuthHint}</div>
          </>
        ) : null}
        {form.type === 'ssh' && !isNetworkDevice ? (
          <>
            <ConnectionSecretField
              id="connection-sudo-password"
              label={t.sudoPassword}
              value={form.sudoPassword}
              hasSavedValue={hasSavedSudoPassword}
              canClear={mode === 'edit'}
              optional
              onChange={(value) => setForm((prev) => ({ ...prev, sudoPassword: value }))}
              onClear={() => setForm((prev) => ({ ...prev, sudoPassword: null }))}
              onUndo={() => setForm((prev) => ({ ...prev, sudoPassword: '' }))}
            />
            <ConnectionSecretField
              id="connection-su-password"
              label={t.suPassword}
              value={form.suPassword}
              hasSavedValue={hasSavedSuPassword}
              canClear={mode === 'edit'}
              optional
              onChange={(value) => setForm((prev) => ({ ...prev, suPassword: value }))}
              onClear={() => setForm((prev) => ({ ...prev, suPassword: null }))}
              onUndo={() => setForm((prev) => ({ ...prev, suPassword: '' }))}
            />
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
              className="flat-button saved-fingerprint-card__action"
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
  )
}
