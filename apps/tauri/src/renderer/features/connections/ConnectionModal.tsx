import { useCallback, useEffect, useState, type FormEvent } from 'react'
import type {
  ConnectionFormMode,
  CreateProfileInput,
  FtpSecurityMode,
  ResourceMonitoringMetric,
  SerialPortInfo,
  SessionType,
  SshConnectionDefaults,
  SshForwardRule
} from '@fileterm/core'
import { normalizeConnectionHost } from '@fileterm/shared'
import { localizeSerialTerminalText, t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'
import { ResourceMonitoringMetricsEditor } from '../common/ResourceMonitoringMetricsEditor'
import { SshPrivateKeyField } from './SshPrivateKeyField'

type SshConnectionSettingKey = keyof SshConnectionDefaults

function ConnectionSecretField({
  id,
  label,
  value,
  hasSavedValue,
  canClear,
  optional = false,
  disabled = false,
  onChange,
  onClear,
  onUndo
}: {
  id: string
  label: string
  value: string | null | undefined
  hasSavedValue: boolean
  canClear: boolean
  optional?: boolean
  disabled?: boolean
  onChange(value: string): void
  onClear(): void
  onUndo(): void
}) {
  const markedForClear = value === null
  const showClearButton = canClear && hasSavedValue && !markedForClear

  return (
    <div className="ssh-secret-field span-2">
      <div className="ssh-secret-field__header">
        <label htmlFor={id}>
          {label}
          {optional ? <span className="ssh-secret-field__optional">{t.optionalField}</span> : null}:
        </label>
        {showClearButton ? (
          <button className="ssh-secret-field__clear" type="button" onClick={onClear} title={t.clearSavedPassword}>
            <AppIcon name="trash" size={13} />
            {t.clearSavedPassword}
          </button>
        ) : markedForClear ? (
          <span className="ssh-secret-field__cleared">
            {t.passwordMarkedForClear}
            <button className="ssh-secret-field__undo" type="button" onClick={onUndo}>
              {t.undoClearSavedPassword}
            </button>
          </span>
        ) : null}
      </div>
      <input
        id={id}
        autoComplete="new-password"
        disabled={disabled || markedForClear}
        placeholder={
          markedForClear
            ? t.passwordMarkedForClear
            : hasSavedValue
              ? t.passwordReplacePlaceholder
              : t.passwordPlaceholder
        }
        type="password"
        value={value ?? ''}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  )
}

function effectiveConnectionSetting<K extends SshConnectionSettingKey>(
  form: CreateProfileInput,
  defaults: SshConnectionDefaults,
  key: K
): SshConnectionDefaults[K] {
  const value = (form as unknown as Record<string, unknown>)[key]
  return (value ?? defaults[key]) as SshConnectionDefaults[K]
}

export function ConnectionModal({
  errorMessage,
  groupOptions,
  connectionDefaults,
  fallbackResourceMonitoringMetrics,
  fallbackResourceMonitoringMetricOrder,
  isSubmitting = false,
  mode,
  form,
  hasSavedPassword = false,
  hasSavedSudoPassword = false,
  hasSavedSuPassword = false,
  setForm,
  onClearHostFingerprint,
  onSubmit,
  onClose,
  standalone = false,
  profiles = []
}: {
  errorMessage: string | null
  groupOptions: string[]
  connectionDefaults: SshConnectionDefaults
  /** Values shown when the profile has no per-connection sidebar metrics yet. */
  fallbackResourceMonitoringMetrics?: ResourceMonitoringMetric[]
  fallbackResourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  isSubmitting?: boolean
  mode: ConnectionFormMode
  form: CreateProfileInput
  hasSavedPassword?: boolean
  hasSavedSudoPassword?: boolean
  hasSavedSuPassword?: boolean
  setForm(value: CreateProfileInput | ((prev: CreateProfileInput) => CreateProfileInput)): void
  onClearHostFingerprint?(): void
  onSubmit(event: FormEvent<HTMLFormElement>): void
  onClose(): void
  standalone?: boolean
  profiles?: import('@fileterm/core').ConnectionProfile[]
}) {
  const [section, setSection] = useState<'ssh' | 'terminal' | 'session-log' | 'proxy' | 'tunnel'>('ssh')
  const [isSelectingSessionLogDirectory, setIsSelectingSessionLogDirectory] = useState(false)
  const [serialPorts, setSerialPorts] = useState<SerialPortInfo[]>([])
  const [isLoadingSerialPorts, setIsLoadingSerialPorts] = useState(false)
  const [serialPortLoadError, setSerialPortLoadError] = useState<string | null>(null)
  const [routingMode, setRoutingMode] = useState<'direct' | 'jump'>(() => (form.jumpProfileId ? 'jump' : 'direct'))
  const supportsProxy = form.type === 'ssh' || form.type === 'telnet'
  const jumpHosts = profiles.filter((profile) => profile.type === 'ssh' && profile.id !== form.name)

  const setSshConnectionSetting = <K extends SshConnectionSettingKey>(key: K, value: SshConnectionDefaults[K]) => {
    setForm((previous) => ({ ...previous, [key]: value }))
  }

  const intervalSettingOptions = [
    { value: '1', label: t.resourceMonitoringEverySecond },
    { value: '5', label: t.resourceMonitoringEvery5Seconds },
    { value: '15', label: t.resourceMonitoringEvery15Seconds },
    { value: '30', label: t.resourceMonitoringEvery30Seconds },
    { value: '60', label: t.resourceMonitoringEvery60Seconds }
  ]

  const refreshSerialPorts = useCallback(async () => {
    const desktopApi = window.fileterm
    if (!desktopApi || form.type !== 'serial') {
      return
    }

    setIsLoadingSerialPorts(true)
    setSerialPortLoadError(null)
    try {
      const ports = await desktopApi.listSerialPorts()
      setSerialPorts(
        [...ports].sort((left, right) => left.portName.localeCompare(right.portName, undefined, { numeric: true }))
      )
    } catch (error) {
      setSerialPorts([])
      setSerialPortLoadError(error instanceof Error ? error.message : String(error))
    } finally {
      setIsLoadingSerialPorts(false)
    }
  }, [form.type])

  useEffect(() => {
    if (form.type !== 'serial') {
      setSerialPorts([])
      setSerialPortLoadError(null)
      return
    }
    void refreshSerialPorts()
  }, [form.type, refreshSerialPorts])

  const serialPortOptions = serialPorts.map((port) => {
    const identity = [port.product, port.manufacturer].filter(Boolean).join(' · ')
    return {
      value: port.portName,
      label: identity ? `${port.portName} — ${identity}` : port.portName
    }
  })

  const chooseSessionLogDirectory = async () => {
    const desktopApi = window.fileterm
    if (!desktopApi || isSelectingSessionLogDirectory) {
      return
    }

    setIsSelectingSessionLogDirectory(true)
    try {
      const directory = await desktopApi.selectLocalDirectory(form.sessionLogDirectory || undefined)
      if (directory) {
        setForm((previous) => ({ ...previous, sessionLogDirectory: directory }))
      }
    } finally {
      setIsSelectingSessionLogDirectory(false)
    }
  }

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
          <button
            className={section === 'session-log' ? 'active' : ''}
            type="button"
            onClick={() => setSection('session-log')}
          >
            {t.sessionLogs}
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
                          { value: 'serial', label: t.serial }
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
                            useEmptyPassword: nextType === 'ssh' ? prev.useEmptyPassword : false,
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
                      <>
                        <label className="span-2">
                          {t.devicePath}:
                          <input
                            placeholder="COM3 / /dev/ttyUSB0 / /dev/cu.usbserial"
                            spellCheck={false}
                            value={form.devicePath ?? ''}
                            onChange={(event) => setForm((prev) => ({ ...prev, devicePath: event.target.value }))}
                          />
                        </label>
                        <div className="span-2 serial-port-picker">
                          <DropdownSelect
                            ariaLabel={t.serialPortSelect}
                            disabled={isLoadingSerialPorts || serialPorts.length === 0}
                            options={
                              serialPortOptions.length
                                ? serialPortOptions
                                : [{ value: '', label: t.serialPortNoDevices, disabled: true }]
                            }
                            placeholder={t.serialPortSelect}
                            value={form.devicePath ?? ''}
                            onChange={(value) => setForm((prev) => ({ ...prev, devicePath: value }))}
                          />
                          <button
                            className="flat-button serial-port-picker__refresh"
                            disabled={isLoadingSerialPorts}
                            title={isLoadingSerialPorts ? t.serialPortRefreshing : t.serialPortRefresh}
                            type="button"
                            onClick={() => void refreshSerialPorts()}
                          >
                            <AppIcon name="refresh" size={14} />
                            <span>{isLoadingSerialPorts ? t.serialPortRefreshing : t.serialPortRefresh}</span>
                          </button>
                        </div>
                        {serialPortLoadError ? (
                          <div className="span-2 ssh-field-hint serial-port-picker__error">
                            {t.serialPortScanFailed}: {localizeSerialTerminalText(serialPortLoadError)}
                          </div>
                        ) : null}
                      </>
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
                          {t.baudRate}:
                          <input
                            inputMode="numeric"
                            value={form.baudRate ?? 115200}
                            onChange={(event) =>
                              setForm((prev) => ({ ...prev, baudRate: Number(event.target.value) || 115200 }))
                            }
                          />
                        </label>
                        <label>
                          {t.dataBits}:
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
                          {t.stopBits}:
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
                          {t.parity}:
                          <DropdownSelect
                            value={form.parity ?? 'none'}
                            options={[
                              { value: 'none', label: t.none },
                              { value: 'odd', label: t.oddParity },
                              { value: 'even', label: t.evenParity },
                              { value: 'mark', label: t.markParity },
                              { value: 'space', label: t.spaceParity }
                            ]}
                            onChange={(value) =>
                              setForm((prev) => ({ ...prev, parity: value as CreateProfileInput['parity'] }))
                            }
                          />
                        </label>
                        <label>
                          {t.flowControl}:
                          <DropdownSelect
                            value={form.flowControl ?? 'none'}
                            options={[
                              { value: 'none', label: t.none },
                              { value: 'hardware', label: t.hardwareFlowControl },
                              { value: 'software', label: t.softwareFlowControl }
                            ]}
                            onChange={(value) =>
                              setForm((prev) => ({
                                ...prev,
                                flowControl: value as CreateProfileInput['flowControl']
                              }))
                            }
                          />
                        </label>
                        <p className="ssh-field-hint span-2">{t.serialParityHint}</p>
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
                    {form.type === 'ssh' && form.authType === 'keyboard-interactive' ? (
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
                    {form.type === 'ssh' ? (
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
                      {form.authType === 'password' ? (
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={effectiveConnectionSetting(form, connectionDefaults, 'useEmptyPassword')}
                              type="checkbox"
                              onChange={(event) => {
                                const nextValue = event.target.checked
                                setSshConnectionSetting('useEmptyPassword', nextValue)
                                if (nextValue) {
                                  setForm((previous) => ({ ...previous, password: '' }))
                                }
                              }}
                            />
                            <span className="advanced-toggle-name">{t.useEmptyPassword}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.useEmptyPasswordHint}</p>
                        </div>
                      ) : null}
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={effectiveConnectionSetting(form, connectionDefaults, 'enableExecChannel')}
                            type="checkbox"
                            onChange={(event) => setSshConnectionSetting('enableExecChannel', event.target.checked)}
                          />
                          <span className="advanced-toggle-name">{t.enableExecChannel}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.enableExecChannelHint}</p>
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring')}
                            type="checkbox"
                            onChange={(event) =>
                              setSshConnectionSetting('enableResourceMonitoring', event.target.checked)
                            }
                          />
                          <span className="advanced-toggle-name">{t.resourceMonitoring}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.resourceMonitoringDescription}</p>
                        <label className="resource-monitoring-interval">
                          <span>{t.resourceMonitoringInterval}</span>
                          <DropdownSelect
                            className="resource-monitoring-interval__select"
                            disabled={!effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring')}
                            options={intervalSettingOptions}
                            value={String(
                              effectiveConnectionSetting(form, connectionDefaults, 'resourceMonitoringIntervalSeconds')
                            )}
                            onChange={(value) =>
                              setSshConnectionSetting(
                                'resourceMonitoringIntervalSeconds',
                                Number(value) as SshConnectionDefaults['resourceMonitoringIntervalSeconds']
                              )
                            }
                          />
                        </label>
                        {form.type === 'ssh' ? (
                          <ResourceMonitoringMetricsEditor
                            metrics={
                              form.resourceMonitoringMetrics ??
                              fallbackResourceMonitoringMetrics ??
                              connectionDefaults.resourceMonitoringMetrics
                            }
                            order={
                              form.resourceMonitoringMetricOrder ??
                              fallbackResourceMonitoringMetricOrder ??
                              connectionDefaults.resourceMonitoringMetricOrder
                            }
                            disabled={
                              !effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring') ||
                              isSubmitting
                            }
                            onMetricsChange={(next) => setSshConnectionSetting('resourceMonitoringMetrics', next)}
                            onOrderChange={(next) => setSshConnectionSetting('resourceMonitoringMetricOrder', next)}
                          />
                        ) : null}
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={effectiveConnectionSetting(form, connectionDefaults, 'legacyAlgorithms')}
                            type="checkbox"
                            onChange={(event) => setSshConnectionSetting('legacyAlgorithms', event.target.checked)}
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
                              checked={effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'none'}
                              name="connection-reconnect-mode"
                              type="radio"
                              onChange={() => setSshConnectionSetting('reconnectMode', 'none')}
                            />
                            <span className="advanced-toggle-name">{t.reconnectNone}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
                        </div>
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={
                                effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'enter'
                              }
                              name="connection-reconnect-mode"
                              type="radio"
                              onChange={() => setSshConnectionSetting('reconnectMode', 'enter')}
                            />
                            <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
                        </div>
                        <div className="advanced-toggle-row">
                          <label className="ssh-checkbox advanced-toggle-label">
                            <input
                              checked={effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'auto'}
                              name="connection-reconnect-mode"
                              type="radio"
                              onChange={() => setSshConnectionSetting('reconnectMode', 'auto')}
                            />
                            <span className="advanced-toggle-name">{t.autoReconnect}</span>
                          </label>
                          <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                        </div>
                      </div>
                    </div>
                    <div className="reconnect-mode-group network-routing-group">
                      <div className="reconnect-mode-group__label network-routing-group__label">
                        <span aria-hidden="true" className="material-symbols-outlined">
                          account_tree
                        </span>
                        {t.networkRouting}
                      </div>
                      <div className="advanced-toggle-list network-routing-list">
                        <div className="advanced-toggle-row network-routing-row">
                          <span className="network-routing-row__name">{t.route}</span>
                          <div className="network-routing-modes" role="radiogroup" aria-label={t.route}>
                            <button
                              aria-checked={routingMode === 'direct'}
                              className={routingMode === 'direct' ? 'is-active' : undefined}
                              onClick={() => {
                                setRoutingMode('direct')
                                setForm((prev) => ({ ...prev, jumpProfileId: undefined }))
                              }}
                              role="radio"
                              type="button"
                            >
                              {t.direct}
                            </button>
                            <button
                              aria-checked={routingMode === 'jump'}
                              className={routingMode === 'jump' ? 'is-active' : undefined}
                              onClick={() => setRoutingMode('jump')}
                              role="radio"
                              type="button"
                            >
                              {t.viaJumpHost}
                            </button>
                          </div>
                        </div>
                        {routingMode === 'jump' && jumpHosts.length ? (
                          <>
                            <label className="advanced-toggle-row network-routing-row">
                              <span className="network-routing-row__name">{t.jumpHost}</span>
                              <DropdownSelect
                                className="network-routing-select"
                                placeholder={t.selectJumpHost}
                                value={form.jumpProfileId ?? ''}
                                options={[
                                  { value: '', label: t.selectJumpHost, disabled: true },
                                  ...jumpHosts.map((profile) => ({
                                    value: profile.id,
                                    label: `${profile.name} (${profile.host})`
                                  }))
                                ]}
                                onChange={(value) =>
                                  setForm((prev) => ({ ...prev, jumpProfileId: value || undefined }))
                                }
                              />
                            </label>
                            <p className="network-routing-hint">{t.jumpHostHint}</p>
                          </>
                        ) : null}
                        {routingMode === 'jump' && !jumpHosts.length ? (
                          <p className="network-routing-empty">{t.noAvailableJumpHost}</p>
                        ) : null}
                      </div>
                    </div>
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
                            <input
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
                            <input
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
                              <input
                                checked={form.dtrOnOpen !== false}
                                type="checkbox"
                                onChange={(event) => setForm((prev) => ({ ...prev, dtrOnOpen: event.target.checked }))}
                              />
                              <span className="advanced-toggle-name">{t.serialDtrOnOpen}</span>
                            </label>
                            <label className="ssh-checkbox advanced-toggle-label">
                              <input
                                checked={form.rtsOnOpen === true}
                                type="checkbox"
                                onChange={(event) => setForm((prev) => ({ ...prev, rtsOnOpen: event.target.checked }))}
                              />
                              <span className="advanced-toggle-name">{t.serialRtsOnOpen}</span>
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
                        <div className="reconnect-mode-group serial-reconnect-mode-group">
                          <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
                          <div className="advanced-toggle-list">
                            <div className="advanced-toggle-row">
                              <label className="ssh-checkbox advanced-toggle-label">
                                <input
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
                                <input
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
                                <input
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
                                  reconnectMaxAttempts: Math.max(
                                    0,
                                    Math.min(4294967295, Number(event.target.value) || 0)
                                  )
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
                      </div>
                    )}
                  </div>
                </fieldset>
              </div>
            ) : null}
            {section === 'session-log' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset narrow">
                  <legend>{t.sessionLogs}</legend>
                  <div className="ssh-grid single">
                    <label className="ssh-checkbox advanced-toggle-label">
                      <input
                        checked={form.sessionLogEnabled === true}
                        onChange={(event) =>
                          setForm((previous) => ({ ...previous, sessionLogEnabled: event.target.checked }))
                        }
                        type="checkbox"
                      />
                      <span className="advanced-toggle-name">{t.autoSaveSessionLog}</span>
                    </label>
                    <p className="ssh-field-hint">{t.autoSaveSessionLogHint}</p>
                    <label>
                      {t.sessionLogDirectory}:
                      <div className="session-log-directory-control">
                        <input
                          readOnly
                          placeholder={t.sessionLogDefaultDirectory}
                          spellCheck={false}
                          value={form.sessionLogDirectory ?? ''}
                        />
                        <button
                          className="flat-button compact"
                          disabled={isSelectingSessionLogDirectory}
                          onClick={() => void chooseSessionLogDirectory()}
                          type="button"
                        >
                          {isSelectingSessionLogDirectory ? t.choosingDirectory : t.chooseDirectory}
                        </button>
                        {form.sessionLogDirectory ? (
                          <button
                            className="flat-button compact"
                            onClick={() => setForm((previous) => ({ ...previous, sessionLogDirectory: '' }))}
                            type="button"
                          >
                            {t.clear}
                          </button>
                        ) : null}
                      </div>
                    </label>
                    <p className="ssh-field-hint">{t.sessionLogPrivacyHint}</p>
                    {form.type === 'serial' ? (
                      <>
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={form.sessionLogIncludeInput === true}
                            onChange={(event) =>
                              setForm((previous) => ({ ...previous, sessionLogIncludeInput: event.target.checked }))
                            }
                            type="checkbox"
                          />
                          <span className="advanced-toggle-name">{t.serialSessionLogIncludeInput}</span>
                        </label>
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={form.sessionLogTimestamps === true}
                            onChange={(event) =>
                              setForm((previous) => ({ ...previous, sessionLogTimestamps: event.target.checked }))
                            }
                            type="checkbox"
                          />
                          <span className="advanced-toggle-name">{t.serialSessionLogTimestamps}</span>
                        </label>
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={form.sessionLogRaw === true}
                            onChange={(event) =>
                              setForm((previous) => ({ ...previous, sessionLogRaw: event.target.checked }))
                            }
                            type="checkbox"
                          />
                          <span className="advanced-toggle-name">{t.serialSessionLogRaw}</span>
                        </label>
                        <p className="ssh-field-hint">{t.serialSessionLogOptionsHint}</p>
                      </>
                    ) : null}
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
