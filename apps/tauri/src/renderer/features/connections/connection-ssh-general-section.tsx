import type { CreateProfileInput, SerialPortInfo, SessionType, SshDeviceMode } from '@fileterm/core'
import { normalizeConnectionHost } from '@fileterm/shared'
import { localizeSerialTerminalText, t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'
import { SelectionControl } from '../common/selection-control'
import { StableButtonContent } from '../common/stable-button-content'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionSshGeneralSection({
  form,
  groupOptions,
  isMacOs,
  isLoadingSerialPorts,
  isNetworkDevice,
  serialDevicePathPlaceholder,
  serialPortLoadError,
  serialPortOptions,
  serialPorts,
  showsNetworkDeviceVendor,
  supportsBuiltInRs485,
  supportsExtendedParity,
  refreshSerialPorts,
  setForm
}: {
  form: CreateProfileInput
  groupOptions: string[]
  isMacOs: boolean
  isLoadingSerialPorts: boolean
  isNetworkDevice: boolean
  serialDevicePathPlaceholder: string
  serialPortLoadError: string | null
  serialPortOptions: Array<{ value: string; label: string }>
  serialPorts: SerialPortInfo[]
  showsNetworkDeviceVendor: boolean
  supportsBuiltInRs485: boolean
  supportsExtendedParity: boolean
  refreshSerialPorts(): Promise<void>
  setForm: ConnectionFormSetter
}) {
  return (
    <fieldset className="ssh-fieldset">
      <legend>{t.general}</legend>
      <div className="ssh-grid ssh-grid-general">
        <label className="span-2">
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
                deviceMode:
                  nextType === 'ssh' ? (prev.type === 'ssh' ? (prev.deviceMode ?? 'server') : 'server') : undefined,
                networkDeviceVendor:
                  nextType === 'ssh'
                    ? prev.type === 'ssh'
                      ? (prev.networkDeviceVendor ?? 'auto')
                      : 'auto'
                    : undefined,
                terminalType:
                  nextType === 'ssh'
                    ? prev.type === 'ssh'
                      ? prev.terminalType
                      : undefined
                    : nextType === 'telnet'
                      ? prev.type === 'telnet'
                        ? (prev.terminalType ?? 'xterm-256color')
                        : 'xterm-256color'
                      : undefined,
                remotePath:
                  nextType === 'ssh'
                    ? prev.type === 'ssh'
                      ? prev.remotePath || '.'
                      : '.'
                    : nextType === 'ftp'
                      ? prev.type === 'ftp'
                        ? prev.remotePath || '/'
                        : '/'
                      : ''
              }))
            }}
          />
        </label>
        {form.type === 'ssh' ? (
          <>
            <label className="span-2">
              {t.sshDeviceMode}:
              <DropdownSelect
                value={form.deviceMode ?? 'server'}
                options={[
                  { value: 'server', label: t.sshDeviceModeServer },
                  { value: 'network-device', label: t.sshDeviceModeNetworkDevice },
                  { value: 'auto', label: t.sshDeviceModeAuto }
                ]}
                onChange={(value) => {
                  const nextMode = value as SshDeviceMode
                  setForm((prev) => {
                    const wasNetworkDevice = prev.type === 'ssh' && prev.deviceMode === 'network-device'
                    const nextIsNetworkDevice = nextMode === 'network-device'
                    return {
                      ...prev,
                      deviceMode: nextMode,
                      networkDeviceVendor: nextIsNetworkDevice
                        ? (prev.networkDeviceVendor ?? 'auto')
                        : prev.networkDeviceVendor,
                      terminalType:
                        nextIsNetworkDevice && !wasNetworkDevice
                          ? 'vt100'
                          : wasNetworkDevice
                            ? nextMode === 'auto'
                              ? undefined
                              : 'xterm-256color'
                            : prev.terminalType
                    }
                  })
                }}
              />
            </label>
            {showsNetworkDeviceVendor ? (
              <label className="span-2">
                {t.networkDeviceVendor}:
                <DropdownSelect
                  value={form.networkDeviceVendor ?? 'auto'}
                  options={[
                    { value: 'auto', label: t.networkDeviceVendorAuto },
                    { value: 'generic', label: t.networkDeviceVendorGeneric },
                    { value: 'cisco', label: t.networkDeviceVendorCisco },
                    { value: 'huawei', label: t.networkDeviceVendorHuawei },
                    { value: 'h3c-comware', label: t.networkDeviceVendorH3c },
                    { value: 'custom', label: t.networkDeviceVendorCustom }
                  ]}
                  onChange={(value) =>
                    setForm((prev) => ({
                      ...prev,
                      networkDeviceVendor: value as CreateProfileInput['networkDeviceVendor']
                    }))
                  }
                />
              </label>
            ) : null}
            <div className="span-2 ssh-field-hint">{t.sshDeviceModeHint}</div>
          </>
        ) : null}
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
          <input value={form.name} onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))} />
        </label>
        {form.type === 'serial' ? (
          <>
            <label className="span-2">
              {t.devicePath}:
              <input
                placeholder={serialDevicePathPlaceholder}
                spellCheck={false}
                value={form.devicePath ?? ''}
                onChange={(event) =>
                  setForm((prev) => ({
                    ...prev,
                    devicePath: event.target.value,
                    deviceSerialNumber: '',
                    deviceVendorId: undefined,
                    deviceProductId: undefined,
                    devicePortType: undefined
                  }))
                }
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
                onChange={(value) => {
                  const selected = serialPorts.find((port) => port.portName === value)
                  setForm((prev) => ({
                    ...prev,
                    devicePath: value,
                    deviceSerialNumber: selected?.serialNumber ?? '',
                    deviceVendorId: selected?.vendorId,
                    deviceProductId: selected?.productId,
                    devicePortType: selected?.portType
                  }))
                }}
              />
              <button
                aria-busy={isLoadingSerialPorts}
                className="flat-button serial-port-picker__refresh"
                disabled={isLoadingSerialPorts}
                title={isLoadingSerialPorts ? t.serialPortRefreshing : t.serialPortRefresh}
                type="button"
                onClick={() => void refreshSerialPorts()}
              >
                <StableButtonContent
                  busy={isLoadingSerialPorts}
                  busyLabel={t.serialPortRefreshing}
                  icon={<AppIcon name="refresh" size={14} />}
                  label={t.serialPortRefresh}
                />
              </button>
            </div>
            {serialPortLoadError ? (
              <div className="span-2 ssh-field-hint serial-port-picker__error">
                {t.serialPortScanFailed}: {localizeSerialTerminalText(serialPortLoadError)}
              </div>
            ) : null}
            {form.deviceSerialNumber || form.deviceVendorId !== undefined || form.deviceProductId !== undefined ? (
              <div className="span-2 ssh-field-hint">{t.serialPortIdentitySaved}</div>
            ) : (
              <div className="span-2 ssh-field-hint">{t.serialPortIdentityMissing}</div>
            )}
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
        {form.type === 'ftp' || (form.type === 'ssh' && !isNetworkDevice) ? (
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
                onChange={(event) => setForm((prev) => ({ ...prev, baudRate: Number(event.target.value) || 115200 }))}
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
                onChange={(value) => setForm((prev) => ({ ...prev, dataBits: Number(value) as 5 | 6 | 7 | 8 }))}
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
                  { value: 'mark', label: t.markParity, disabled: !supportsExtendedParity },
                  { value: 'space', label: t.spaceParity, disabled: !supportsExtendedParity }
                ]}
                onChange={(value) => setForm((prev) => ({ ...prev, parity: value as CreateProfileInput['parity'] }))}
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
            <label>
              {t.serialRs485Mode}:
              <DropdownSelect
                value={form.rs485Mode ?? 'none'}
                options={[
                  { value: 'none', label: t.serialRs485None },
                  {
                    value: 'half-duplex',
                    label: t.serialRs485HalfDuplex,
                    disabled: !supportsBuiltInRs485
                  }
                ]}
                onChange={(value) =>
                  setForm((prev) => ({
                    ...prev,
                    rs485Mode: value as CreateProfileInput['rs485Mode']
                  }))
                }
              />
            </label>
            {form.flowControl === 'software' ? (
              <p className="ssh-field-hint span-2">{t.serialTransferSoftwareFlowControl}</p>
            ) : null}
            {form.rs485Mode === 'half-duplex' ? (
              <>
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.rs485RtsOnSend !== false}
                    type="checkbox"
                    onChange={(event) => setForm((prev) => ({ ...prev, rs485RtsOnSend: event.target.checked }))}
                  />
                  <span className="advanced-toggle-name">{t.serialRs485RtsOnSend}</span>
                </label>
                <label>
                  {t.serialRs485DelayBefore}:
                  <input
                    inputMode="numeric"
                    min={0}
                    max={60000}
                    type="number"
                    value={form.rs485DelayRtsBeforeSendMs ?? 0}
                    onChange={(event) =>
                      setForm((prev) => ({
                        ...prev,
                        rs485DelayRtsBeforeSendMs: Math.max(0, Math.min(60000, Number(event.target.value) || 0))
                      }))
                    }
                  />
                </label>
                <p className="ssh-field-hint full">{t.telnetTerminalTypeHint}</p>
                <label>
                  {t.serialRs485DelayAfter}:
                  <input
                    inputMode="numeric"
                    min={0}
                    max={60000}
                    type="number"
                    value={form.rs485DelayRtsAfterSendMs ?? 0}
                    onChange={(event) =>
                      setForm((prev) => ({
                        ...prev,
                        rs485DelayRtsAfterSendMs: Math.max(0, Math.min(60000, Number(event.target.value) || 0))
                      }))
                    }
                  />
                </label>
              </>
            ) : null}
            <p className="ssh-field-hint span-2">{t.serialParityHint}</p>
            {!supportsExtendedParity && (form.parity === 'mark' || form.parity === 'space') ? (
              <p className="ssh-field-hint span-2">
                {isMacOs ? t.serialParityMacRequirement : t.serialParityUnsupported}
              </p>
            ) : null}
            <p className="ssh-field-hint span-2">
              {isMacOs && form.rs485Mode === 'half-duplex' ? t.serialRs485MacHint : t.serialRs485Hint}
            </p>
            {form.rs485Mode === 'half-duplex' && form.flowControl === 'hardware' ? (
              <p className="ssh-field-hint span-2">{t.serialRs485FlowConflict}</p>
            ) : !supportsBuiltInRs485 && form.rs485Mode === 'half-duplex' ? (
              <p className="ssh-field-hint span-2">{t.serialRs485Unsupported}</p>
            ) : null}
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
  )
}
