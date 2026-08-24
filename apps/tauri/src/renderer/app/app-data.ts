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
  legacyAlgorithms: false,
  secure: false,
  securityMode: 'none',
  proxy: { type: 'none', host: '', port: 1080, username: '' },
  proxyPassword: '',
  sudoPassword: '',
  suPassword: '',
  forwards: [],
  devicePath: '',
  baudRate: 115200,
  dataBits: 8,
  stopBits: 1,
  parity: 'none',
  flowControl: 'none',
  newlineMode: 'none',
  inputMode: 'text',
  outputMode: 'text',
  localEcho: false
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
    encoding: profile.type === 'ssh' ? (profile.encoding ?? 'UTF-8') : 'UTF-8',
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
    reconnectMode:
      profile.type === 'ssh'
        ? sshConnectionSettings.reconnectMode
        : profile.type === 'serial'
          ? (profile.reconnectMode ?? 'none')
          : 'none',
    legacyAlgorithms: profile.type === 'ssh' ? sshConnectionSettings.legacyAlgorithms : false,
    secure: profile.type === 'ftp' ? profile.secure : false,
    securityMode: profile.type === 'ftp' ? (profile.securityMode ?? (profile.secure ? 'explicit' : 'none')) : 'none',
    proxy:
      profile.type === 'ssh' || profile.type === 'telnet'
        ? (profile.proxy ?? { type: 'none', host: '', port: 1080 })
        : { type: 'none', host: '', port: 1080 },
    proxyPassword: profile.type === 'ssh' || profile.type === 'telnet' ? (profile.proxy?.password ?? '') : '',
    sudoPassword: '',
    suPassword: '',
    jumpProfileId: profile.type === 'ssh' ? profile.jumpProfileId : undefined,
    forwards: profile.type === 'ssh' ? (profile.forwards ?? []) : [],
    disableShellIntegration: profile.type === 'ssh' ? profile.disableShellIntegration : false,
    devicePath: profile.type === 'serial' ? profile.devicePath : '',
    baudRate: profile.type === 'serial' ? profile.baudRate : 115200,
    dataBits: profile.type === 'serial' ? profile.dataBits : 8,
    stopBits: profile.type === 'serial' ? profile.stopBits : 1,
    parity: profile.type === 'serial' ? profile.parity : 'none',
    flowControl: profile.type === 'serial' ? profile.flowControl : 'none',
    newlineMode: profile.type === 'serial' ? (profile.newlineMode ?? 'none') : 'none',
    inputMode: profile.type === 'serial' ? (profile.inputMode ?? 'text') : 'text',
    outputMode: profile.type === 'serial' ? (profile.outputMode ?? 'text') : 'text',
    localEcho: profile.type === 'serial' ? (profile.localEcho ?? false) : false
  }
}
