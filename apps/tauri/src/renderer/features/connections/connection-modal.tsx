import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import type {
  ConnectionFormMode,
  CreateProfileInput,
  ResourceMonitoringMetric,
  SerialPortInfo,
  SshConnectionDefaults
} from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/close-button'
import { FeedbackText } from '../common/feedback-text'
import { StableButtonContent } from '../common/stable-button-content'
import { waitForMinimumBusyDuration } from '../common/operation-timing'
import { ConnectionProxySection } from './connection-proxy-section'
import { ConnectionSessionLogSection } from './connection-session-log-section'
import { ConnectionSshSection } from './connection-ssh-section'
import { ConnectionTerminalSection } from './connection-terminal-section'
import { ConnectionTunnelSection } from './connection-tunnel-section'
import { isValidFtpCertificateFingerprint, type SshConnectionSettingKey } from './connection-modal-utils'

const CONNECTION_TEST_RETRY_COOLDOWN_MS = 5000

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
  onTestConnection,
  onSubmit,
  onClose,
  standalone = false,
  profiles = [],
  editingProfileId = null
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
  onTestConnection(): Promise<boolean>
  onSubmit(event: FormEvent<HTMLFormElement>): void
  onClose(): void
  standalone?: boolean
  profiles?: import('@fileterm/core').ConnectionProfile[]
  editingProfileId?: string | null
}) {
  const [section, setSection] = useState<'ssh' | 'terminal' | 'session-log' | 'proxy' | 'tunnel'>('ssh')
  const [isSelectingSessionLogDirectory, setIsSelectingSessionLogDirectory] = useState(false)
  const [serialPorts, setSerialPorts] = useState<SerialPortInfo[]>([])
  const [isLoadingSerialPorts, setIsLoadingSerialPorts] = useState(false)
  const [serialPortLoadError, setSerialPortLoadError] = useState<string | null>(null)
  const [serialValidationError, setSerialValidationError] = useState<string | null>(null)
  const [isTestingConnection, setIsTestingConnection] = useState(false)
  const [isTestRetryCoolingDown, setIsTestRetryCoolingDown] = useState(false)
  const [connectionTestSucceeded, setConnectionTestSucceeded] = useState(false)
  const [routingMode, setRoutingMode] = useState<'direct' | 'jump'>(() => (form.jumpProfileId ? 'jump' : 'direct'))
  const serialPortRefreshInFlightRef = useRef(false)
  const supportsProxy = form.type === 'ssh' || form.type === 'telnet' || form.type === 'ftp'
  const isNetworkDevice = form.type === 'ssh' && form.deviceMode === 'network-device'
  const showsNetworkDeviceVendor = form.type === 'ssh' && (isNetworkDevice || form.deviceMode === 'auto')
  const platform = window.fileterm?.platform
  const isMacOs = platform === 'darwin'
  const supportsBuiltInRs485 = ['linux', 'darwin'].includes(platform ?? '') && form.flowControl !== 'hardware'
  const supportsExtendedParity = ['linux', 'win32'].includes(platform ?? '') || (isMacOs && form.dataBits === 7)
  const jumpHosts = profiles.filter((profile) => profile.type === 'ssh' && profile.id !== editingProfileId)
  const serialDevicePathPlaceholder =
    platform === 'darwin'
      ? '/dev/cu.usbserial / /dev/tty.usbserial'
      : platform === 'win32'
        ? 'COM3 / COM4'
        : platform === 'linux'
          ? '/dev/ttyUSB0 / /dev/ttyACM0'
          : 'COM3 / /dev/ttyUSB0 / /dev/cu.usbserial'

  useEffect(() => {
    setConnectionTestSucceeded(false)
  }, [form])

  const isFormBusy = isSubmitting || isTestingConnection
  const isTestBusy = isFormBusy || isTestRetryCoolingDown

  const connectionTestFeedback = serialValidationError
    ? {
        message: serialValidationError,
        tone: 'error' as const
      }
    : errorMessage
      ? { message: errorMessage, tone: 'error' as const }
      : connectionTestSucceeded
        ? {
            message: t.connectionTestSuccess,
            tone: 'success' as const
          }
        : null

  const handleTestConnection = async () => {
    if (isTestBusy) {
      return
    }

    setIsTestingConnection(true)
    setConnectionTestSucceeded(false)
    setSerialValidationError(null)
    try {
      setConnectionTestSucceeded(await onTestConnection())
    } finally {
      setIsTestingConnection(false)
      setIsTestRetryCoolingDown(true)
    }
  }

  useEffect(() => {
    if (!isTestRetryCoolingDown) {
      return
    }

    const timeoutId = window.setTimeout(() => setIsTestRetryCoolingDown(false), CONNECTION_TEST_RETRY_COOLDOWN_MS)
    return () => window.clearTimeout(timeoutId)
  }, [isTestRetryCoolingDown])

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
    if (!desktopApi || form.type !== 'serial' || serialPortRefreshInFlightRef.current) {
      return
    }

    serialPortRefreshInFlightRef.current = true
    const refreshStartedAt = performance.now()
    setIsLoadingSerialPorts(true)
    setSerialPortLoadError(null)
    try {
      const ports = await desktopApi.listSerialPorts()
      setSerialPorts(
        [...ports].sort((left, right) => left.portName.localeCompare(right.portName, undefined, { numeric: true }))
      )
    } catch (error) {
      setSerialPortLoadError(error instanceof Error ? error.message : String(error))
    } finally {
      await waitForMinimumBusyDuration(refreshStartedAt)
      serialPortRefreshInFlightRef.current = false
      setIsLoadingSerialPorts(false)
    }
  }, [form.type])

  useEffect(() => {
    if (form.type !== 'serial') {
      setSerialPorts([])
      setSerialPortLoadError(null)
      return
    }
    const stopWatching = window.fileterm?.onSerialPortsChanged((ports) => {
      setSerialPorts(
        [...ports].sort((left, right) => left.portName.localeCompare(right.portName, undefined, { numeric: true }))
      )
      setSerialPortLoadError(null)
    })
    void refreshSerialPorts()
    const timer = window.setInterval(() => void refreshSerialPorts(), 3000)
    return () => {
      stopWatching?.()
      window.clearInterval(timer)
    }
  }, [form.type, refreshSerialPorts])

  const serialPortOptions = serialPorts.map((port) => {
    const identity = [port.product, port.manufacturer].filter(Boolean).join(' · ')
    return {
      value: port.portName,
      label: identity ? `${port.portName} — ${identity}` : port.portName
    }
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    if (form.type === 'ftp' && form.securityMode === 'implicit' && form.proxy?.type && form.proxy.type !== 'none') {
      event.preventDefault()
      setSection('proxy')
      setSerialValidationError(t.ftpImplicitProxyUnsupported)
      return
    }
    if (
      form.type === 'ftp' &&
      form.securityMode !== 'none' &&
      form.certificateFingerprint?.trim() &&
      !isValidFtpCertificateFingerprint(form.certificateFingerprint)
    ) {
      event.preventDefault()
      setSection('ssh')
      setSerialValidationError(t.ftpCertificateFingerprintInvalid)
      return
    }
    if (form.type === 'serial') {
      if (form.parity === 'mark' || form.parity === 'space') {
        if (!supportsExtendedParity) {
          event.preventDefault()
          setSection('ssh')
          setSerialValidationError(isMacOs ? t.serialParityMacRequirement : t.serialParityUnsupported)
          return
        }
      }
      if (form.rs485Mode === 'half-duplex' && form.flowControl === 'hardware') {
        event.preventDefault()
        setSection('ssh')
        setSerialValidationError(t.serialRs485FlowConflict)
        return
      }
      if (form.rs485Mode === 'half-duplex' && !supportsBuiltInRs485) {
        event.preventDefault()
        setSection('ssh')
        setSerialValidationError(t.serialRs485Unsupported)
        return
      }
    }
    setSerialValidationError(null)
    onSubmit(event)
  }

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
          <CloseButton disabled={isFormBusy} onClick={onClose} />
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
        <form aria-busy={isFormBusy} className="ssh-form-shell" onSubmit={handleSubmit}>
          <fieldset
            className="connection-form-submit-lock"
            disabled={isFormBusy}
            style={{ border: 0, display: 'contents', margin: 0, padding: 0 }}
          >
            {section === 'ssh' ? (
              <ConnectionSshSection
                connectionDefaults={connectionDefaults}
                fallbackResourceMonitoringMetrics={fallbackResourceMonitoringMetrics}
                fallbackResourceMonitoringMetricOrder={fallbackResourceMonitoringMetricOrder}
                form={form}
                groupOptions={groupOptions}
                hasSavedPassword={hasSavedPassword}
                hasSavedSuPassword={hasSavedSuPassword}
                hasSavedSudoPassword={hasSavedSudoPassword}
                intervalSettingOptions={intervalSettingOptions}
                isMacOs={isMacOs}
                isLoadingSerialPorts={isLoadingSerialPorts}
                isNetworkDevice={isNetworkDevice}
                isSubmitting={isSubmitting}
                jumpHosts={jumpHosts}
                mode={mode}
                onClearHostFingerprint={onClearHostFingerprint}
                refreshSerialPorts={refreshSerialPorts}
                routingMode={routingMode}
                serialDevicePathPlaceholder={serialDevicePathPlaceholder}
                serialPortLoadError={serialPortLoadError}
                serialPortOptions={serialPortOptions}
                serialPorts={serialPorts}
                setForm={setForm}
                setRoutingMode={setRoutingMode}
                setSshConnectionSetting={setSshConnectionSetting}
                showsNetworkDeviceVendor={showsNetworkDeviceVendor}
                supportsBuiltInRs485={supportsBuiltInRs485}
                supportsExtendedParity={supportsExtendedParity}
              />
            ) : null}
            {section === 'terminal' ? (
              <ConnectionTerminalSection form={form} isNetworkDevice={isNetworkDevice} setForm={setForm} />
            ) : null}
            {section === 'session-log' ? (
              <ConnectionSessionLogSection
                chooseSessionLogDirectory={chooseSessionLogDirectory}
                form={form}
                isSelectingSessionLogDirectory={isSelectingSessionLogDirectory}
                setForm={setForm}
              />
            ) : null}
            {section === 'proxy' && supportsProxy ? <ConnectionProxySection form={form} setForm={setForm} /> : null}
            {section === 'tunnel' && form.type === 'ssh' ? (
              <ConnectionTunnelSection form={form} setForm={setForm} />
            ) : null}
            <div className="form-actions ssh-actions">
              {connectionTestFeedback ? (
                <FeedbackText
                  className="connection-test-feedback"
                  message={connectionTestFeedback.message}
                  tone={connectionTestFeedback.tone}
                />
              ) : null}
              <button className="flat-button" disabled={isFormBusy} onClick={onClose} type="button">
                {t.cancel}
              </button>
              <button
                aria-busy={isTestingConnection}
                className="flat-button"
                disabled={isTestBusy}
                onClick={() => {
                  void handleTestConnection()
                }}
                type="button"
              >
                <StableButtonContent busy={isTestingConnection} label={t.test} />
              </button>
              <button aria-busy={isSubmitting} className="primary-button" disabled={isFormBusy} type="submit">
                <StableButtonContent busy={isSubmitting} label={t.save} />
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
