import {
  DEFAULT_RESOURCE_MONITORING_METRICS,
  DEFAULT_RESOURCE_MONITORING_METRIC_ORDER,
  DEFAULT_SSH_CONNECTION_DEFAULTS
} from '@fileterm/core'
import type {
  ConnectionProfile,
  CreateProfileInput,
  LocalFileItem,
  SshConnectionDefaults,
  SshConnectionOverrides,
  SshProfile,
  WorkspaceSnapshot
} from '@fileterm/core'
import { t } from '../i18n'

export const emptyState: WorkspaceSnapshot = {
  profiles: [],
  folders: [],
  commandFolders: [],
  commandTemplates: [],
  tabs: [],
  activeTabId: null,
  transfers: [],
  sessions: {}
}

export const localPreviewFiles: LocalFileItem[] = []

export const previewLocalPath = ''

export const previewState: WorkspaceSnapshot = emptyState

export const defaultForm: CreateProfileInput = {
  type: 'ssh',
  name: '',
  host: '',
  port: 22,
  username: '',
  group: t.defaultGroup,
  remotePath: '/',
  note: '',
  sessionLogEnabled: false,
  sessionLogDirectory: '',
  password: '',
  useEmptyPassword: false,
  privateKeyId: '',
  privateKeyPath: '',
  passphrase: '',
  trustedHostFingerprint: '',
  authType: 'password',
  encoding: 'UTF-8',
  backspaceKey: 'ASCII',
  deleteKey: 'VT220',
  enableExecChannel: true,
  enableResourceMonitoring: true,
  resourceMonitoringIntervalSeconds: 1,
  resourceMonitoringMetrics: DEFAULT_RESOURCE_MONITORING_METRICS,
  resourceMonitoringMetricOrder: DEFAULT_RESOURCE_MONITORING_METRIC_ORDER,
  reconnectMode: 'none',
  reconnectMaxAttempts: 0,
  reconnectInitialDelayMs: 2000,
  reconnectMaxDelayMs: 30000,
  keepaliveEnabled: true,
  keepaliveIntervalSeconds: 30,
  keepaliveMaxMisses: 3,
  connectTimeoutSeconds: 30,
  operationTimeoutSeconds: 60,
  sftpEnabled: true,
  legacyAlgorithms: false,
  secure: false,
  securityMode: 'none',
  transferMode: 'passive',
  certificateFingerprint: '',
  terminalType: 'xterm-256color',
  crNul: true,
  loginScript: '',
  proxy: { type: 'none', host: '', port: 1080, username: '' },
  proxyPassword: '',
  sudoPassword: '',
  suPassword: '',
  forwards: [],
  devicePath: '',
  deviceSerialNumber: '',
  deviceVendorId: undefined,
  deviceProductId: undefined,
  devicePortType: undefined,
  baudRate: 115200,
  dataBits: 8,
  stopBits: 1,
  parity: 'none',
  flowControl: 'none',
  newlineMode: 'none',
  inputMode: 'text',
  lineMode: false,
  outputMode: 'text',
  localEcho: false,
  dtrOnOpen: true,
  rtsOnOpen: false,
  dtrOnClose: undefined,
  rtsOnClose: undefined,
  serialCharDelayMs: 0,
  serialLineDelayMs: 0,
  serialReceiveIdleTimeoutMs: 5000,
  serialWriteTimeoutMs: 30000,
  serialTransferMaxFileBytes: 4 * 1024 * 1024 * 1024,
  serialTransferMaxBatchBytes: 16 * 1024 * 1024 * 1024,
  serialTransferMaxFiles: 128,
  rs485Mode: 'none',
  rs485RtsOnSend: true,
  rs485DelayRtsBeforeSendMs: 0,
  rs485DelayRtsAfterSendMs: 0,
  sessionLogIncludeInput: false,
  sessionLogTimestamps: false,
  sessionLogRaw: false
}

function connectionOverridesForProfile(profile: SshProfile): SshConnectionOverrides {
  if (profile.connectionOverrides) {
    return { ...profile.connectionOverrides }
  }

  // Profiles written before global defaults existed stored these values
  // directly. Treat present legacy fields as explicit local overrides so
  // introducing global defaults does not silently change existing hosts.
  const overrides: SshConnectionOverrides = {}
  if (profile.useEmptyPassword !== undefined) overrides.useEmptyPassword = profile.useEmptyPassword
  if (profile.enableExecChannel !== undefined) overrides.enableExecChannel = profile.enableExecChannel
  if (profile.enableResourceMonitoring !== undefined) {
    overrides.enableResourceMonitoring = profile.enableResourceMonitoring
  }
  if (profile.resourceMonitoringIntervalSeconds !== undefined) {
    overrides.resourceMonitoringIntervalSeconds = profile.resourceMonitoringIntervalSeconds
  }
  if (profile.reconnectMode !== undefined) overrides.reconnectMode = profile.reconnectMode
  if (profile.legacyAlgorithms !== undefined) overrides.legacyAlgorithms = profile.legacyAlgorithms
  return overrides
}

function connectionSettingsForProfile(profile: SshProfile, defaults: SshConnectionDefaults): SshConnectionDefaults {
  const overrides = connectionOverridesForProfile(profile)
  return {
    useEmptyPassword: overrides.useEmptyPassword ?? profile.useEmptyPassword ?? defaults.useEmptyPassword,
    enableExecChannel: overrides.enableExecChannel ?? profile.enableExecChannel ?? defaults.enableExecChannel,
    enableResourceMonitoring:
      overrides.enableResourceMonitoring ?? profile.enableResourceMonitoring ?? defaults.enableResourceMonitoring,
    resourceMonitoringIntervalSeconds:
      overrides.resourceMonitoringIntervalSeconds ??
      profile.resourceMonitoringIntervalSeconds ??
      defaults.resourceMonitoringIntervalSeconds,
    // Metrics are handled by direct pass-through in profileToForm; these
    // entries only satisfy the full-defaults shape for the form helper.
    resourceMonitoringMetrics: defaults.resourceMonitoringMetrics,
    resourceMonitoringMetricOrder: defaults.resourceMonitoringMetricOrder,
    reconnectMode: overrides.reconnectMode ?? profile.reconnectMode ?? defaults.reconnectMode,
    legacyAlgorithms: overrides.legacyAlgorithms ?? profile.legacyAlgorithms ?? defaults.legacyAlgorithms
  }
}

export function profileToForm(
  profile: ConnectionProfile,
  connectionDefaults: SshConnectionDefaults = DEFAULT_SSH_CONNECTION_DEFAULTS
): CreateProfileInput {
  const sshConnectionSettings =
    profile.type === 'ssh' ? connectionSettingsForProfile(profile, connectionDefaults) : DEFAULT_SSH_CONNECTION_DEFAULTS
  return {
    type: profile.type,
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    group: profile.group,
    remotePath: profile.remotePath,
    note: profile.note ?? '',
    sessionLogEnabled: profile.sessionLogEnabled ?? false,
    sessionLogDirectory: profile.sessionLogDirectory ?? '',
    password: profile.type === 'ssh' || profile.type === 'ftp' ? (profile.password ?? '') : '',
    useEmptyPassword: profile.type === 'ssh' ? sshConnectionSettings.useEmptyPassword : false,
    trustedHostFingerprint: profile.type === 'ssh' ? (profile.trustedHostFingerprint ?? '') : '',
    authType: profile.type === 'ssh' ? (profile.authType === 'system' ? 'password' : profile.authType) : 'password',
    privateKeyId: profile.type === 'ssh' ? (profile.privateKeyId ?? '') : '',
    privateKeyPath: profile.type === 'ssh' ? (profile.privateKeyPath ?? '') : '',
    passphrase: profile.type === 'ssh' ? (profile.passphrase ?? '') : '',
    encoding:
      profile.type === 'ssh' || profile.type === 'telnet' || profile.type === 'serial'
        ? (profile.encoding ?? 'UTF-8')
        : 'UTF-8',
    backspaceKey: profile.type === 'ssh' ? (profile.backspaceKey ?? 'ASCII') : 'ASCII',
    deleteKey: profile.type === 'ssh' ? (profile.deleteKey ?? 'VT220') : 'VT220',
    enableExecChannel: profile.type === 'ssh' ? sshConnectionSettings.enableExecChannel : true,
    enableResourceMonitoring: profile.type === 'ssh' ? sshConnectionSettings.enableResourceMonitoring : true,
    resourceMonitoringIntervalSeconds:
      profile.type === 'ssh' ? sshConnectionSettings.resourceMonitoringIntervalSeconds : 1,
    // Sidebar metrics stay pass-through: a profile without saved values keeps
    // its legacy fallback instead of being rewritten with current defaults.
    resourceMonitoringMetrics: profile.type === 'ssh' ? profile.resourceMonitoringMetrics : undefined,
    resourceMonitoringMetricOrder: profile.type === 'ssh' ? profile.resourceMonitoringMetricOrder : undefined,
    reconnectMode: profile.type === 'ssh' ? sshConnectionSettings.reconnectMode : (profile.reconnectMode ?? 'none'),
    reconnectMaxAttempts: profile.reconnectMaxAttempts ?? 0,
    reconnectInitialDelayMs: profile.reconnectInitialDelayMs ?? 2000,
    reconnectMaxDelayMs: profile.reconnectMaxDelayMs ?? 30000,
    keepaliveEnabled: profile.keepaliveEnabled ?? true,
    keepaliveIntervalSeconds: profile.keepaliveIntervalSeconds ?? 30,
    keepaliveMaxMisses: profile.keepaliveMaxMisses ?? 3,
    connectTimeoutSeconds: profile.connectTimeoutSeconds ?? 30,
    operationTimeoutSeconds: profile.operationTimeoutSeconds ?? 60,
    sftpEnabled: profile.type === 'ssh' ? (profile.sftpEnabled ?? true) : false,
    legacyAlgorithms: profile.type === 'ssh' ? sshConnectionSettings.legacyAlgorithms : false,
    secure: profile.type === 'ftp' ? profile.secure : false,
    securityMode: profile.type === 'ftp' ? (profile.securityMode ?? (profile.secure ? 'explicit' : 'none')) : 'none',
    transferMode: profile.type === 'ftp' ? (profile.transferMode ?? 'passive') : 'passive',
    certificateFingerprint: profile.type === 'ftp' ? (profile.certificateFingerprint ?? '') : '',
    terminalType: profile.type === 'telnet' ? (profile.terminalType ?? 'xterm-256color') : 'xterm-256color',
    crNul: profile.type === 'telnet' ? (profile.crNul ?? true) : true,
    loginScript: profile.type === 'telnet' ? (profile.loginScript ?? '') : '',
    proxy:
      profile.type === 'ssh' || profile.type === 'telnet' || profile.type === 'ftp'
        ? (profile.proxy ?? { type: 'none', host: '', port: 1080 })
        : { type: 'none', host: '', port: 1080 },
    proxyPassword:
      profile.type === 'ssh' || profile.type === 'telnet' || profile.type === 'ftp'
        ? (profile.proxy?.password ?? '')
        : '',
    sudoPassword: '',
    suPassword: '',
    jumpProfileId: profile.type === 'ssh' ? profile.jumpProfileId : undefined,
    forwards: profile.type === 'ssh' ? (profile.forwards ?? []) : [],
    disableShellIntegration: profile.type === 'ssh' ? profile.disableShellIntegration : false,
    devicePath: profile.type === 'serial' ? (profile.devicePath ?? '') : '',
    deviceSerialNumber: profile.type === 'serial' ? (profile.deviceSerialNumber ?? '') : '',
    deviceVendorId: profile.type === 'serial' ? profile.deviceVendorId : undefined,
    deviceProductId: profile.type === 'serial' ? profile.deviceProductId : undefined,
    devicePortType: profile.type === 'serial' ? profile.devicePortType : undefined,
    baudRate: profile.type === 'serial' ? (profile.baudRate ?? 115200) : 115200,
    dataBits: profile.type === 'serial' ? (profile.dataBits ?? 8) : 8,
    stopBits: profile.type === 'serial' ? (profile.stopBits ?? 1) : 1,
    parity: profile.type === 'serial' ? (profile.parity ?? 'none') : 'none',
    flowControl: profile.type === 'serial' ? (profile.flowControl ?? 'none') : 'none',
    newlineMode:
      profile.type === 'serial'
        ? (profile.newlineMode ?? 'none')
        : profile.type === 'telnet'
          ? (profile.newlineMode ?? 'crlf')
          : 'none',
    inputMode: profile.type === 'serial' ? (profile.inputMode ?? 'text') : 'text',
    lineMode: profile.type === 'serial' ? (profile.lineMode ?? false) : false,
    outputMode: profile.type === 'serial' ? (profile.outputMode ?? 'text') : 'text',
    localEcho: profile.type === 'serial' ? (profile.localEcho ?? false) : false,
    dtrOnOpen: profile.type === 'serial' ? (profile.dtrOnOpen ?? true) : true,
    rtsOnOpen: profile.type === 'serial' ? (profile.rtsOnOpen ?? false) : false,
    dtrOnClose: profile.type === 'serial' ? profile.dtrOnClose : undefined,
    rtsOnClose: profile.type === 'serial' ? profile.rtsOnClose : undefined,
    serialCharDelayMs: profile.type === 'serial' ? (profile.serialCharDelayMs ?? 0) : 0,
    serialLineDelayMs: profile.type === 'serial' ? (profile.serialLineDelayMs ?? 0) : 0,
    serialReceiveIdleTimeoutMs: profile.type === 'serial' ? (profile.serialReceiveIdleTimeoutMs ?? 5000) : 5000,
    serialWriteTimeoutMs: profile.type === 'serial' ? (profile.serialWriteTimeoutMs ?? 30000) : 30000,
    serialTransferMaxFileBytes:
      profile.type === 'serial'
        ? (profile.serialTransferMaxFileBytes ?? 4 * 1024 * 1024 * 1024)
        : 4 * 1024 * 1024 * 1024,
    serialTransferMaxBatchBytes:
      profile.type === 'serial'
        ? (profile.serialTransferMaxBatchBytes ?? 16 * 1024 * 1024 * 1024)
        : 16 * 1024 * 1024 * 1024,
    serialTransferMaxFiles: profile.type === 'serial' ? (profile.serialTransferMaxFiles ?? 128) : 128,
    rs485Mode: profile.type === 'serial' ? (profile.rs485Mode ?? 'none') : 'none',
    rs485RtsOnSend: profile.type === 'serial' ? (profile.rs485RtsOnSend ?? true) : true,
    rs485DelayRtsBeforeSendMs: profile.type === 'serial' ? (profile.rs485DelayRtsBeforeSendMs ?? 0) : 0,
    rs485DelayRtsAfterSendMs: profile.type === 'serial' ? (profile.rs485DelayRtsAfterSendMs ?? 0) : 0,
    sessionLogIncludeInput: profile.type === 'serial' ? (profile.sessionLogIncludeInput ?? false) : false,
    sessionLogTimestamps: profile.type === 'serial' ? (profile.sessionLogTimestamps ?? false) : false,
    sessionLogRaw: profile.type === 'serial' ? (profile.sessionLogRaw ?? false) : false
  }
}
