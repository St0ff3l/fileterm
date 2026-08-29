export type SessionType = 'ssh' | 'ftp' | 'telnet' | 'serial'

/** Runtime workspace sessions may also be a local shell; connection profiles never are. */
export type WorkspaceSessionType = SessionType | 'local'

/** Optional launch overrides for an isolated local PTY tab. */
export interface LocalTerminalLaunchOptions {
  /** Shell executable name or absolute path. Defaults to the platform shell. */
  shell?: string
  /** User-visible label for this one local terminal tab. Defaults to Local Terminal. */
  title?: string
  /** Initial working directory. Defaults to the current user's home directory. */
  cwd?: string
  /** Additional shell options. These are not persisted in a connection profile. */
  args?: string[]
  /** Environment overrides applied only to the new local PTY process. */
  env?: Record<string, string>
}

export type FtpSecurityMode = 'none' | 'explicit' | 'implicit'
export type FtpTransferMode = 'passive' | 'active'
export type TelnetTerminalType = 'xterm-256color' | 'vt100' | 'vt220' | 'ansi'
/** SSH session target. `auto` resolves conservatively from the SSH identification. */
export type SshDeviceMode = 'auto' | 'server' | 'network-device'
/** Terminal type requested by an SSH PTY. Network devices commonly prefer vt100. */
export type SshTerminalType = 'xterm-256color' | 'xterm' | 'vt100' | 'vt220' | 'ansi' | 'linux'
/** Optional vendor family hint for network-device terminal behavior. */
export type SshNetworkDeviceVendor = 'auto' | 'generic' | 'cisco' | 'huawei' | 'h3c-comware' | 'custom'
export type TelnetNewlineMode = 'none' | 'lf' | 'cr' | 'crlf'

export type TabLayout = 'terminal-file' | 'file-only' | 'terminal-only'

/** 分屏方向：row = 左右分（垂直分屏），column = 上下分（水平分屏） */
export type SplitDirection = 'row' | 'column'

/** 在分屏树中移动焦点的方向。 */
export type PaneFocusDirection = 'left' | 'right' | 'up' | 'down'

/** 焦点终端的字号操作，不涉及 WebView 页面缩放。 */
export type TerminalZoomOperation = 'in' | 'out' | 'reset'

/** 分屏树节点。leaf 引用一个真实 WorkspaceTab id；split 递归持有子节点。 */
export type PaneNode =
  | { kind: 'leaf'; tabId: string }
  | {
      kind: 'split'
      direction: SplitDirection
      children: PaneNode[]
      /** 每个子节点占比，长度与 children 一致，和为 1。拖拽 resize 时更新。 */
      weights: number[]
    }

export type TabStatus = 'idle' | 'connecting' | 'connected' | 'error' | 'closed'

export interface BaseEntity {
  id: string
  name: string
  parentId?: string
  order?: number
}

export interface ConnectionFolder extends BaseEntity {
  type: 'folder'
  isExpanded?: boolean
}

export type LocalNetworkShareConnectionResult =
  | {
      kind: 'connected'
      path: string
      shares: string[]
    }
  | {
      kind: 'select-share'
      path: string
      shares: string[]
    }

export interface CommandFolder extends BaseEntity {
  type: 'command-folder'
}

export interface CommandTemplate extends BaseEntity {
  type: 'command-template'
  command: string
  description?: string
  appendCarriageReturn: boolean
}

export interface BaseProfile extends BaseEntity {
  type: SessionType
  /** Serial profiles keep these inert placeholders; devicePath remains their only endpoint. */
  host: string
  port: number
  username: string
  remotePath: string
  group: string
  lastUsedAt?: number
  /** Persisted per-device terminal output logging preference. */
  sessionLogEnabled?: boolean
  /** Absolute directory for automatic session logs; empty uses FileTerm's default folder. */
  sessionLogDirectory?: string
  /** Non-secret indicator for redacted desktop snapshots. */
  hasSavedPassword?: boolean
  /** Non-secret indicator for a saved privileged-command password. */
  hasSavedSudoPassword?: boolean
  /** Non-secret indicator for a saved `su` password. */
  hasSavedSuPassword?: boolean
  /** Retry policy shared by network sessions. Zero/undefined means unlimited. */
  reconnectMode?: 'none' | 'enter' | 'auto'
  reconnectMaxAttempts?: number
  reconnectInitialDelayMs?: number
  reconnectMaxDelayMs?: number
  /** Transport keepalive policy shared by SSH, Telnet and FTP. */
  keepaliveEnabled?: boolean
  keepaliveIntervalSeconds?: number
  keepaliveMaxMisses?: number
  /** Upper bounds for connection setup and one protocol operation. */
  connectTimeoutSeconds?: number
  operationTimeoutSeconds?: number
}

export type NetworkProfile = BaseProfile

export interface ProxyConfig {
  type: 'none' | 'socks5' | 'http'
  host: string
  port: number
  username?: string
  /** Secret: persisted only by the main-process profile repository. */
  password?: string
}

export interface SshForwardRule {
  id: string
  name?: string
  kind: 'local' | 'remote' | 'dynamic'
  bindHost: string
  bindPort: number
  targetHost?: string
  targetPort?: number
  autoStart: boolean
}

/** A forward rule plus its state in one live SSH workspace tab. */
export interface SshTunnelSnapshot extends SshForwardRule {
  status: 'stopped' | 'starting' | 'running' | 'stopping' | 'error'
  error?: string
  runtimeOnly?: boolean
}

export interface ConnectionCapabilities {
  terminal: boolean
  files: boolean
  resourceMonitoring: boolean
  shellIntegration: boolean
  fileAccess: boolean
  tunnels: boolean
}

export interface RemoteDiskSpace {
  availableBytes: number
  totalBytes: number
}

/** Protocol-level file features discovered from the current remote session. */
export interface RemoteFileCapabilities {
  protocol: 'sftp' | 'ftp'
  protocolVersion?: string
  extensions: string[]
  checksumAlgorithms: string[]
  diskSpace?: RemoteDiskSpace
  serverCopy: boolean
  symlink: boolean
  hardlink: boolean
}

export type SshAuthType = 'password' | 'privateKey' | 'system' | 'keyboard-interactive'

/** 资源监控采集间隔，单位为秒。 */
export type ResourceMonitoringIntervalSeconds = 1 | 5 | 15 | 30 | 60

/** 侧栏可显示的资源监控项。采集总开关仍由 enableResourceMonitoring 控制。 */
export type ResourceMonitoringMetric =
  | 'load'
  | 'cpu'
  | 'memory'
  | 'swap'
  | 'disk'
  | 'gpu'
  | 'gpuMemory'
  | 'gpuTemperature'
  | 'gpuPower'
  | 'processes'
  | 'network'

export const DEFAULT_RESOURCE_MONITORING_METRICS: ResourceMonitoringMetric[] = [
  'load',
  'cpu',
  'memory',
  'swap',
  'disk',
  'processes',
  'network'
]

/** Canonical order for all resource-monitoring cards, including disabled items. */
export const DEFAULT_RESOURCE_MONITORING_METRIC_ORDER: ResourceMonitoringMetric[] = [
  'load',
  'cpu',
  'memory',
  'swap',
  'disk',
  'gpu',
  'gpuMemory',
  'gpuTemperature',
  'gpuPower',
  'processes',
  'network'
]

/** SSH connection behavior shared by newly created connections. */
export interface SshConnectionDefaults {
  useEmptyPassword: boolean
  enableExecChannel: boolean
  enableResourceMonitoring: boolean
  resourceMonitoringIntervalSeconds: ResourceMonitoringIntervalSeconds
  /** Sidebar metric cards enabled by default on newly created connections. */
  resourceMonitoringMetrics: ResourceMonitoringMetric[]
  /** Default sidebar card order (including disabled cards) for new connections. */
  resourceMonitoringMetricOrder: ResourceMonitoringMetric[]
  reconnectMode: 'none' | 'enter' | 'auto'
  legacyAlgorithms: boolean
}

/** Legacy per-connection override metadata retained for persisted profile compatibility. */
export type SshConnectionOverrides = Partial<SshConnectionDefaults>

export const DEFAULT_SSH_CONNECTION_DEFAULTS: SshConnectionDefaults = {
  useEmptyPassword: false,
  enableExecChannel: true,
  enableResourceMonitoring: true,
  resourceMonitoringIntervalSeconds: 1,
  resourceMonitoringMetrics: [...DEFAULT_RESOURCE_MONITORING_METRICS],
  resourceMonitoringMetricOrder: [...DEFAULT_RESOURCE_MONITORING_METRIC_ORDER],
  reconnectMode: 'none',
  legacyAlgorithms: false
}

/** Default local terminal shell command stored separately for each desktop OS. */
export type LocalTerminalPlatform = 'win32' | 'darwin' | 'linux'

export interface LocalTerminalShellPreferences {
  win32: string
  darwin: string
  linux: string
}

/** An installed shell candidate discovered on the current desktop platform. */
export interface LocalTerminalShellOption {
  shell: string
  label: string
  path: string
}

export const DEFAULT_LOCAL_TERMINAL_SHELLS: LocalTerminalShellPreferences = {
  win32: 'pwsh.exe',
  darwin: '/bin/zsh',
  linux: '/bin/bash'
}

/** Saved connections exposed to externally launched MCP Agents such as Codex and Claude. */
export type McpConnectionScope = 'all-saved-connections' | 'selected-connections'

/** Whether an external MCP Agent may request state-changing operations. */
export type McpOperationPolicy = 'read-only' | 'approved-operations' | 'full-access'

/** Non-secret local policy for an external MCP Agent connection. */
export interface McpAgentPreferences {
  connectionScope: McpConnectionScope
  operationPolicy: McpOperationPolicy
  /** Saved connection profiles that an Agent may access in selected mode. */
  allowedProfileIds: string[]
}

export const DEFAULT_MCP_AGENT_PREFERENCES: McpAgentPreferences = {
  connectionScope: 'selected-connections',
  allowedProfileIds: [],
  operationPolicy: 'approved-operations'
}

export type McpAgentClientId = 'claude-code' | 'codex-cli' | 'opencode'

/** Local client discovery result. Detection reads PATH and known install locations, never runs the client. */
export interface McpAgentClientStatus {
  id: McpAgentClientId
  label: string
  command: string
  available: boolean
  path?: string
  registrationCommand: string
}

/** Backend-generated configuration help for local stdio MCP clients. */
export interface McpAgentSetup {
  filetermCommand: string
  clients: McpAgentClientStatus[]
}

export interface SshProfile extends NetworkProfile {
  type: 'ssh'
  /** Explicit session target. Missing keeps legacy server behavior; auto resolves after handshake. */
  deviceMode?: SshDeviceMode
  /** PTY terminal type sent during SSH shell setup. */
  terminalType?: SshTerminalType
  /** Optional vendor hint retained for network-device profiles. */
  networkDeviceVendor?: SshNetworkDeviceVendor
  username: string
  authType: SshAuthType
  note?: string
  password?: string
  /** Explicitly authenticate with an empty SSH password instead of using a saved password. */
  useEmptyPassword?: boolean
  privateKeyId?: string
  privateKeyPath?: string
  passphrase?: string
  trustedHostFingerprint?: string
  /** Whether the SFTP subsystem should be opened for this SSH session. */
  sftpEnabled?: boolean
  remotePath: string
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
  enableResourceMonitoring?: boolean
  resourceMonitoringIntervalSeconds?: ResourceMonitoringIntervalSeconds
  /** Sidebar metric cards enabled for this connection. Absent = legacy global preference fallback. */
  resourceMonitoringMetrics?: ResourceMonitoringMetric[]
  /** Sidebar card order (including disabled cards) for this connection. */
  resourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  reconnectMode?: 'none' | 'enter' | 'auto'
  connectionOverrides?: SshConnectionOverrides
  proxy?: ProxyConfig
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
  /** 兼容老服务器：追加 SHA-1 类 MAC/KEX 算法到偏好列表末尾（SHA-2 仍优先） */
  legacyAlgorithms?: boolean
}

export interface FtpProfile extends NetworkProfile {
  type: 'ftp'
  username: string
  note?: string
  password?: string
  secure: boolean
  securityMode?: FtpSecurityMode
  transferMode?: FtpTransferMode
  proxy?: ProxyConfig
  /** Optional SHA-256 pin for the FTPS server certificate. CA validation remains required. */
  certificateFingerprint?: string
  remotePath: string
}

export interface TelnetProfile extends NetworkProfile {
  type: 'telnet'
  note?: string
  encoding?: string
  terminalType?: TelnetTerminalType
  newlineMode?: TelnetNewlineMode
  /** Send CR NUL instead of a bare CR when Telnet line mode requires it. */
  crNul?: boolean
  /** Optional command lines sent after Telnet negotiation completes. */
  loginScript?: string
  proxy?: ProxyConfig
}

export interface SerialProfile extends BaseProfile {
  type: 'serial'
  devicePath: string
  /** Stable USB identity used to recover from a changed /dev/tty path. */
  deviceSerialNumber?: string
  deviceVendorId?: number
  deviceProductId?: number
  devicePortType?: SerialPortType
  baudRate: number
  dataBits: 5 | 6 | 7 | 8
  stopBits: 1 | 2
  parity: 'none' | 'odd' | 'even' | 'mark' | 'space'
  flowControl: 'none' | 'hardware' | 'software'
  encoding?: string
  /** How line endings typed into a serial terminal are sent to the device. */
  newlineMode?: SerialNewlineMode
  /** Whether typed input is encoded as text or hexadecimal bytes. */
  inputMode?: SerialInputMode
  /** Buffer text until Enter before writing it to the serial device. */
  lineMode?: boolean
  /** Whether received bytes are rendered as text or hexadecimal bytes. */
  outputMode?: SerialOutputMode
  /** Echo transmitted bytes locally before a device echo arrives. */
  localEcho?: boolean
  /** Initial modem-control line levels applied after opening the port. */
  dtrOnOpen?: boolean
  rtsOnOpen?: boolean
  /** Optional modem-control levels applied before the port is closed. */
  dtrOnClose?: boolean
  rtsOnClose?: boolean
  /** Delay between transmitted bytes and completed lines, in milliseconds. */
  serialCharDelayMs?: number
  serialLineDelayMs?: number
  /** Idle timeout for Raw receive mode, in milliseconds. */
  serialReceiveIdleTimeoutMs?: number
  /** Timeout for a blocked serial write/flush, in milliseconds. */
  serialWriteTimeoutMs?: number
  /** Maximum size of one received serial-transfer file, in bytes. */
  serialTransferMaxFileBytes?: number
  /** Maximum aggregate size of one received serial-transfer batch, in bytes. */
  serialTransferMaxBatchBytes?: number
  /** Maximum number of files accepted in one received serial-transfer batch. */
  serialTransferMaxFiles?: number
  /** Linux RS-485 transceiver mode; unsupported platforms fail closed. */
  rs485Mode?: 'none' | 'half-duplex'
  rs485RtsOnSend?: boolean
  rs485DelayRtsBeforeSendMs?: number
  rs485DelayRtsAfterSendMs?: number
  /** Include TX bytes in automatic serial session logs. */
  sessionLogIncludeInput?: boolean
  /** Prefix automatic session-log records with unambiguous local timestamps and offset. */
  sessionLogTimestamps?: boolean
  /** Capture serial RX/TX records as raw uppercase hexadecimal bytes. */
  sessionLogRaw?: boolean
  reconnectMode?: 'none' | 'enter' | 'auto'
  note?: string
}

export type SerialNewlineMode = 'none' | 'lf' | 'cr' | 'crlf'
export type SerialInputMode = 'text' | 'hex'
export type SerialOutputMode = 'text' | 'hex'
export type SerialControlAction =
  'set-dtr' | 'set-rts' | 'pulse-dtr' | 'pulse-rts' | 'send-break' | 'clear-buffers' | 'reset' | 'status'

export interface SerialLineStatus {
  dtr: boolean | null
  rts: boolean | null
  dtrReadback: boolean
  rtsReadback: boolean
  rtsManual: boolean
  cts: boolean | null
  dsr: boolean | null
  ring: boolean | null
  carrierDetect: boolean | null
}

export type SerialTransferDirection = 'send' | 'receive'
export type SerialTransferMode = 'raw' | 'xmodem' | 'ymodem' | 'zmodem' | 'kermit'

export interface SerialTransferResult {
  bytesTransferred: number
  localPath: string
}

export interface SerialTransferProgress {
  tabId: string
  direction: SerialTransferDirection
  mode: SerialTransferMode
  localPath: string
  status: 'running' | 'completed' | 'failed' | 'canceled'
  bytesTransferred: number
  totalBytes?: number
  speedBytesPerSecond?: number
  block?: number
  message?: string
}

export type ConnectionProfile = SshProfile | FtpProfile | TelnetProfile | SerialProfile

export const getConnectionCapabilities = (profile: {
  type: SessionType
  deviceMode?: SshDeviceMode
}): ConnectionCapabilities => {
  if (profile.type === 'ssh') {
    if (profile.deviceMode === 'network-device') {
      return {
        terminal: true,
        files: false,
        resourceMonitoring: false,
        shellIntegration: false,
        fileAccess: false,
        tunnels: true
      }
    }
    return {
      terminal: true,
      files: true,
      resourceMonitoring: true,
      shellIntegration: true,
      fileAccess: true,
      tunnels: true
    }
  }
  if (profile.type === 'ftp') {
    return {
      terminal: false,
      files: true,
      resourceMonitoring: false,
      shellIntegration: false,
      fileAccess: false,
      tunnels: false
    }
  }
  return {
    terminal: true,
    files: false,
    resourceMonitoring: false,
    shellIntegration: false,
    fileAccess: false,
    tunnels: false
  }
}

export interface WorkspaceTab {
  id: string
  sessionType: WorkspaceSessionType
  profileId: string
  title: string
  layout: TabLayout
  status: TabStatus
  /** 分屏树根节点；普通 tab 无此字段。只有分屏的根 tab 持有。 */
  paneRoot?: PaneNode
  /** 分屏 leaf 所属的顶层 workspace tab；leaf 永不显示在顶栏。 */
  paneRootTabId?: string
}

export type WorkspaceSessionTabEvent =
  | {
      type: 'status-changed'
      tabId: string
      status: TabStatus
      summary: string
      connected: boolean
    }
  | {
      type: 'disconnected'
      tabId: string
      summary: string
    }
  | {
      type: 'cwd-changed'
      tabId: string
      shellCwd: string
      remotePath: string
      followShellCwd: boolean
    }
  | {
      type: 'file-access-changed'
      tabId: string
      source: 'shell' | 'manual'
      fileAccessMode: 'user' | 'root'
      shellUser?: string
      sudoUser?: string
    }
  | {
      type: 'ssh-handshake'
      tabId: string
      phase: 'connected' | 'failed'
      summary: string
    }

export interface RemoteFileItem {
  path: string
  name: string
  type: 'file' | 'folder'
  /** Whether the remote entry is a symbolic link to the effective type. */
  isSymlink?: boolean
  modified: string
  size: string
  permission?: string
  ownerGroup?: string
}

export interface LocalFileItem extends RemoteFileItem {
  path: string
}

export type TransferStatus =
  'queued' | 'running' | 'paused' | 'interrupted' | 'verifying' | 'finalizing' | 'done' | 'failed' | 'canceled'

export interface TransferFileIdentity {
  size: number
  modifiedAt?: number
}

export type TransferManifestEntryStatus = 'pending' | 'running' | 'done'

export interface TransferManifestEntry {
  relativePath: string
  sourcePath: string
  destinationPath: string
  partialPath: string
  stagingPath?: string
  sourceIdentity: TransferFileIdentity
  status: TransferManifestEntryStatus
  transferredBytes: number
}

export interface TransferManifest {
  version: 1
  directories: string[]
  files: TransferManifestEntry[]
}

export interface TransferTask {
  id: string
  direction: 'upload' | 'download'
  name: string
  progress: number
  status: TransferStatus
  message?: string
  speed?: string
  transferredBytes?: number
  totalBytes?: number
  tabId?: string
  profileId?: string
  sessionType?: SessionType
  fileAccessMode?: 'user' | 'root'
  targetType?: RemoteFileItem['type']
  sourcePath?: string
  destinationPath?: string
  partialPath?: string
  stagingPath?: string
  sourceIdentity?: TransferFileIdentity
  manifest?: TransferManifest
  resumable?: boolean
  retryAttempt?: number
  cleanupPending?: boolean
  createdAt?: number
  updatedAt?: number
}

export interface TransferProgress {
  percent: number
  transferredBytes?: number
  totalBytes?: number
  message?: string
}

export interface TransferTargetOptions {
  targetName?: string
}

export interface TransferFileOptions {
  resumeOffset?: number
  signal?: AbortSignal
  stagingPath?: string
}

export interface RemoteFileStat {
  size: number
  modifiedAt?: number
}

export interface PermissionChangeOptions {
  mode: string
  recursive?: boolean
  applyTo?: 'all' | 'files' | 'directories'
}

export interface FileContentSnapshot {
  path: string
  name: string
  content: string
  source: 'local' | 'remote'
  tabId?: string
  encoding?: string
}

export interface FileEditorWindowInput {
  source: 'local' | 'remote'
  path: string
  name: string
  tabId?: string
  encoding?: string
}

export interface DirectorySnapshot<TItem> {
  path: string
  items: TItem[]
}

export interface SidebarProcessItem {
  pid: number
  user: string
  memory: string
  cpu: string
  command: string
  elapsedSeconds: number
}

export interface NetworkSamplePoint {
  rx: number
  tx: number
}

export interface NetworkRates {
  rx: string
  tx: string
}

export type RemoteSystemPlatform = 'linux' | 'busybox' | 'darwin' | 'freebsd' | 'windows' | 'unknown'

export interface RawResourceUsageBreakdown {
  totalBytes: number
  usedBytes: number
  availableBytes: number
  percent: number
}

export interface RawNetworkInterfaceMetrics {
  name: string
  rxBytes: number
  txBytes: number
  rxBytesPerSecond: number
  txBytesPerSecond: number
}

export interface SystemIdentity {
  osName: string
  kernelName: string
  kernelVersion: string
  architecture: string
  hostname: string
}

export interface CpuInfoRow {
  model: string
  cores: number
  frequencyMHz: string
  cache: string
  bogomips: string
}

export interface GpuInfoRow {
  model: string
  vendor: string
  driver: string
  memory: string
  usagePercent?: number
  memoryUsed?: string
  memoryPercent?: number
  temperatureCelsius?: number
  powerUsage?: string
  powerLimit?: string
}

export interface CpuUsageBreakdown {
  user: number
  system: number
  nice: number
  idle: number
  ioWait: number
  irq: number
  softIrq: number
  steal: number
}

export interface ResourceUsageBreakdown {
  total: string
  used: string
  available: string
  percent: number
}

export interface NetworkInterfaceRow {
  name: string
  txTotal: string
  rxTotal: string
  txRate: string
  rxRate: string
}

export interface FileSystemRow {
  name: string
  size: string
  used: string
  usagePercent: string
  available: string
  mountPoint: string
}

export interface SystemMetrics {
  platform?: RemoteSystemPlatform
  ip: string
  uptime: string
  uptimeSeconds?: number
  load: string
  loadUnit?: 'busy-logical-processors'
  identity: SystemIdentity
  cpuPercent: number
  cpuUsage: CpuUsageBreakdown
  cpuInfoRows: CpuInfoRow[]
  gpuInfoRows: GpuInfoRow[]
  memoryPercent: number
  memoryUsage: string
  memoryAppUsage?: string
  memoryCacheUsage?: string
  memoryKernelUsage?: string
  memoryBreakdown: ResourceUsageBreakdown
  memoryRaw?: RawResourceUsageBreakdown & {
    appBytes?: number
    cacheBytes?: number
    kernelBytes?: number
  }
  swapPercent: number
  swapUsage: string
  swapBreakdown: ResourceUsageBreakdown
  swapRaw?: RawResourceUsageBreakdown
  diskRows: Array<{ path: string; usage: string }>
  fileSystemRows: FileSystemRow[]
  networkInterfaces: string[]
  activeNetworkInterface: string
  networkRates: NetworkRates
  networkSamples: NetworkSamplePoint[]
  networkInterfaceRows: NetworkInterfaceRow[]
  networkRatesByInterface?: Record<string, NetworkRates>
  networkSamplesByInterface?: Record<string, NetworkSamplePoint[]>
  networkRawByInterface?: Record<string, RawNetworkInterfaceMetrics>
  topProcesses: SidebarProcessItem[]
}

export function mergeSystemMetricsHistory(
  previousMetrics: SystemMetrics | undefined,
  nextMetrics: SystemMetrics,
  historyLimit = 600
): SystemMetrics {
  const nextPoint = nextMetrics.networkSamples.at(-1) ?? { rx: 0, tx: 0 }
  const previousSamples = previousMetrics?.networkSamples ?? []
  const previousByInterface = previousMetrics?.networkSamplesByInterface ?? {}
  const nextByInterface = nextMetrics.networkSamplesByInterface ?? {}
  const mergedByInterface = Object.fromEntries(
    Object.entries(nextByInterface).map(([name, samples]) => {
      const nextInterfacePoint = samples.at(-1) ?? { rx: 0, tx: 0 }
      const previousInterfaceSamples = previousByInterface[name] ?? []
      return [name, [...previousInterfaceSamples, nextInterfacePoint].slice(-historyLimit)]
    })
  )

  // System collectors emit a full snapshot most of the time, but a partial
  // refresh can temporarily omit the filesystem sections. Do not let that
  // transient empty payload erase the last usable disk information in the
  // sidebar.
  const diskRows = nextMetrics.diskRows?.length ? nextMetrics.diskRows : (previousMetrics?.diskRows ?? [])
  const fileSystemRows = nextMetrics.fileSystemRows?.length
    ? nextMetrics.fileSystemRows
    : (previousMetrics?.fileSystemRows ?? [])

  return {
    ...nextMetrics,
    diskRows,
    fileSystemRows,
    networkSamples: [...previousSamples, nextPoint].slice(-historyLimit),
    networkSamplesByInterface: mergedByInterface
  }
}

export interface SessionSnapshot {
  profileId: string
  /** Monotonic terminal-target identity; unchanged by ordinary output chunks. */
  aiSessionRevision?: string
  /** Effective SSH mode after banner resolution; absent while auto is connecting. */
  deviceMode?: SshDeviceMode
  accessHost?: string
  summary: string
  terminalTranscript?: string
  remotePath: string
  shellCwd?: string
  shellUser?: string
  followShellCwd?: boolean
  remoteFilesLoading?: boolean
  remoteFiles: RemoteFileItem[]
  /**
   * SSH shell sessions may be usable even when the server refuses the SFTP
   * subsystem. Keep that distinction in the shared snapshot so the renderer
   * can disable only file actions instead of presenting the whole session as
   * disconnected.
   */
  sftpUnavailableReason?: string
  fileAccessMode?: 'user' | 'root'
  sudoUser?: string
  hasReusableSudoAuth?: boolean
  /** 登录用户（首次 OSC 1337 RemoteUser= 观察值或 profile.username） */
  loginUser?: string
  connected?: boolean
  systemMetrics?: SystemMetrics
  capabilities?: ConnectionCapabilities
  remoteCapabilities?: RemoteFileCapabilities
  reconnectMode?: 'none' | 'enter' | 'auto'
}

export interface WorkspaceSnapshot {
  /** Monotonic runtime revision used to discard late, stale IPC snapshots. */
  workspaceRevision?: number
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  tabs: WorkspaceTab[]
  activeTabId: string | null
  transfers: TransferTask[]
  sessions: Record<string, SessionSnapshot>
  /** 分屏 root tabId -> 当前活跃 leaf tabId。用于终端输入/文件操作/命令发送定位。 */
  activePaneTabIdByRoot?: Record<string, string>
}

export interface SessionMetricsUpdate {
  tabId: string
  systemMetrics?: SystemMetrics
  mode?: 'replace' | 'append'
}

export interface ConnectionLibrarySnapshot {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
}

export interface SshKeyMetadata {
  id: string
  name: string
  note?: string
  algorithm: string
  fingerprint: string
  encrypted: boolean
  importedAt: number
  usageCount: number
}

export interface ImportSshKeyInput {
  sourcePath?: string
  /**
   * Transient private-key text entered in the import dialog. It must never be
   * persisted in a connection profile or workspace snapshot.
   */
  content?: string
  note?: string
}

export type SshKeyImportSource = Pick<ImportSshKeyInput, 'sourcePath' | 'content'>

export interface SshKeyFileSelection {
  sourcePath: string
  fileName: string
  existingKey?: SshKeyMetadata
}

export interface SshKeyImportResult {
  key: SshKeyMetadata
  duplicate: boolean
}

export type ImportedFontFormat = 'ttf' | 'otf'

export interface ImportedFont {
  id: string
  family: string
  format: ImportedFontFormat
  fileName: string
  contentHash: string
  importedAt: number
}

export interface ConnectionImportPreviewItem {
  id?: string
  sourceLabel?: string
  name: string
  type: SessionType
  host?: string
  port?: number
  username?: string
  status: 'ready' | 'skipped' | 'invalid'
  reason?: string
  unsupportedFields?: string[]
  conflictProfileId?: string
  input?: CreateProfileInput
}

export interface ConnectionImportResult {
  imported: number
  overwritten?: number
  skipped: number
  failed: number
  items: ConnectionImportPreviewItem[]
}

export type ConnectionImportConflictStrategy = 'skip' | 'overwrite' | 'create'

export interface ConnectionImportPlan {
  id: string
  items: ConnectionImportPreviewItem[]
}

export interface ConnectionImportOptions {
  selectedItemIds?: string[]
  conflictStrategy?: ConnectionImportConflictStrategy
}

export type ConnectionExportFormat = 'fileterm' | 'compatible'

export interface WebDavSyncConfig {
  enabled: boolean
  url: string
  username?: string
  remotePath: string
  allowInsecureTls?: boolean
  lastSyncedAt?: string
  lastEtag?: string
}

export type BackupUploadMode = 'overwrite-cloud' | 'merge-cloud'
export type BackupDownloadMode = 'overwrite-local' | 'merge-local'

export interface WebDavSyncResult {
  action: 'test' | 'upload' | 'download'
  message: string
  imported?: number
  updated?: number
  replaced?: number
  skipped?: number
  legacyPlaintext?: boolean
}

export type S3BackupProvider = 'custom' | 'cloudflare-r2' | 'bitiful-s4'

export interface S3BackupConfig {
  enabled: boolean
  provider: S3BackupProvider
  endpoint: string
  region: string
  bucket: string
  remotePath: string
  pathStyleAccessEnabled: boolean
  accessKeyId?: string
  hasSavedSecret: boolean
  lastSyncedAt?: string
  lastEtag?: string
}

export type S3BackupConfigInput = Pick<
  S3BackupConfig,
  'enabled' | 'provider' | 'endpoint' | 'region' | 'bucket' | 'remotePath' | 'pathStyleAccessEnabled' | 'accessKeyId'
> & {
  secretAccessKey?: string
}

export interface S3BackupResult {
  action: 'test' | 'upload' | 'download'
  message: string
  imported?: number
  updated?: number
  replaced?: number
  skipped?: number
  legacyPlaintext?: boolean
}

export interface SshKeyMetadata {
  id: string
  name: string
  note?: string
  algorithm: string
  fingerprint: string
  encrypted: boolean
  importedAt: number
  usageCount: number
}

export interface ImportSshKeyInput {
  sourcePath?: string
  /**
   * Transient private-key text entered in the import dialog. It must never be
   * persisted in a connection profile or workspace snapshot.
   */
  content?: string
  note?: string
}

export interface SshKeyFileSelection {
  sourcePath: string
  fileName: string
  existingKey?: SshKeyMetadata
}

export interface SshKeyImportResult {
  key: SshKeyMetadata
  duplicate: boolean
}

export interface CreateProfileInput {
  type: SessionType
  name: string
  host: string
  port: number
  username: string
  group: string
  remotePath: string
  note?: string
  sessionLogEnabled?: boolean
  sessionLogDirectory?: string
  password?: string | null
  /** Explicitly authenticate with an empty SSH password instead of using a saved password. */
  useEmptyPassword?: boolean
  privateKeyId?: string
  privateKeyPath?: string
  passphrase?: string
  authType?: SshAuthType
  trustedHostFingerprint?: string
  secure?: boolean
  securityMode?: FtpSecurityMode
  transferMode?: FtpTransferMode
  certificateFingerprint?: string
  encoding?: string
  /** SSH PTY terminal type; Telnet uses the compatible subset of these values. */
  terminalType?: SshTerminalType
  deviceMode?: SshDeviceMode
  networkDeviceVendor?: SshNetworkDeviceVendor
  crNul?: boolean
  loginScript?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
  enableResourceMonitoring?: boolean
  resourceMonitoringIntervalSeconds?: ResourceMonitoringIntervalSeconds
  /** Sidebar metric cards enabled for this connection. Undefined keeps the profile-level fallback. */
  resourceMonitoringMetrics?: ResourceMonitoringMetric[]
  /** Sidebar card order (including disabled cards) for this connection. */
  resourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  reconnectMode?: 'none' | 'enter' | 'auto'
  reconnectMaxAttempts?: number
  reconnectInitialDelayMs?: number
  reconnectMaxDelayMs?: number
  keepaliveEnabled?: boolean
  keepaliveIntervalSeconds?: number
  keepaliveMaxMisses?: number
  connectTimeoutSeconds?: number
  operationTimeoutSeconds?: number
  sftpEnabled?: boolean
  connectionOverrides?: SshConnectionOverrides
  proxy?: ProxyConfig
  proxyPassword?: string
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
  /** Transient only; persisted by Rust in profile-secrets.json. */
  sudoPassword?: string | null
  /** Transient only; persisted by Rust in profile-secrets.json. */
  suPassword?: string | null
  /** 兼容老服务器：追加 SHA-1 类 MAC/KEX 算法到偏好列表末尾（SHA-2 仍优先） */
  legacyAlgorithms?: boolean
  devicePath?: string
  deviceSerialNumber?: string
  deviceVendorId?: number
  deviceProductId?: number
  devicePortType?: SerialPortType
  baudRate?: number
  dataBits?: 5 | 6 | 7 | 8
  stopBits?: 1 | 2
  parity?: SerialProfile['parity']
  flowControl?: SerialProfile['flowControl']
  newlineMode?: SerialProfile['newlineMode'] | TelnetNewlineMode
  inputMode?: SerialProfile['inputMode']
  lineMode?: boolean
  outputMode?: SerialProfile['outputMode']
  localEcho?: boolean
  dtrOnOpen?: boolean
  rtsOnOpen?: boolean
  dtrOnClose?: boolean
  rtsOnClose?: boolean
  serialCharDelayMs?: number
  serialLineDelayMs?: number
  serialReceiveIdleTimeoutMs?: number
  serialWriteTimeoutMs?: number
  serialTransferMaxFileBytes?: number
  serialTransferMaxBatchBytes?: number
  serialTransferMaxFiles?: number
  rs485Mode?: SerialProfile['rs485Mode']
  rs485RtsOnSend?: boolean
  rs485DelayRtsBeforeSendMs?: number
  rs485DelayRtsAfterSendMs?: number
  sessionLogIncludeInput?: boolean
  sessionLogTimestamps?: boolean
  sessionLogRaw?: boolean
}

export interface SshHostVerificationRequest {
  requestId: string
  tabId: string
  kind: 'host-verification'
  profileId: string
  host: string
  port: number
  fingerprint: string
  knownFingerprint?: string
}

export interface SshCredentialsPromptRequest {
  requestId: string
  tabId: string
  kind: 'credentials'
  profileId: string
  host: string
  port: number
  username?: string
  passwordRequired: boolean
  reason: 'missing-username' | 'missing-password'
}

export interface SshKeyPassphrasePromptRequest {
  requestId: string
  tabId: string
  kind: 'key-passphrase'
  profileId: string
  keyId: string
  keyName: string
  reason: 'required' | 'invalid-saved'
}

export interface SshKeyboardInteractivePrompt {
  prompt: string
  echo: boolean
}

export interface SshKeyboardInteractiveRequest {
  requestId: string
  tabId: string
  kind: 'keyboard-interactive'
  profileId: string
  host: string
  port: number
  name: string
  instructions: string
  prompts: SshKeyboardInteractivePrompt[]
}

export interface SshKeyPassphrasePromptRequest {
  requestId: string
  tabId: string
  kind: 'key-passphrase'
  profileId: string
  keyId: string
  keyName: string
  reason: 'required' | 'invalid-saved'
}

export type SshInteractionRequest =
  | SshHostVerificationRequest
  | SshCredentialsPromptRequest
  | SshKeyPassphrasePromptRequest
  | SshKeyboardInteractiveRequest

/** One-shot privileged-command input. Values must never be persisted or sent
 * to an AI provider; the desktop bridge forwards them only to Rust. */
export interface RemoteExecCredentials {
  sudoPassword?: string
  suPassword?: string
  saveSudoPassword?: boolean
  saveSuPassword?: boolean
}

/** One-time local prompt for a privileged command when no saved credential exists. */
export interface SudoPasswordRequest {
  requestId: string
  tabId: string
  kind: 'sudo' | 'su'
  host: string
  shellUser?: string
  cwd?: string
  command: string
}

export type BackupPasswordOperation = 'upload' | 'download'

/** One-time password request for a cross-device WebDAV/S3 backup. */
export interface BackupPasswordRequest {
  requestId: string
  operation: BackupPasswordOperation
  provider: 'WebDAV' | 'S3'
}

/** Non-secret session and remote-backup security state exposed to the renderer. */
export interface SecuritySettings {
  lockEnabled: boolean
  idleLockMinutes: number
  hasLockPassword: boolean
  hasBackupPassword: boolean
}

/** Secret values are write-only from the renderer's point of view. */
export interface SecuritySettingsInput {
  lockEnabled?: boolean
  idleLockMinutes?: number
  currentLockPassword?: string
  currentBackupPassword?: string
  lockPassword?: string
  backupPassword?: string
  clearLockPassword?: boolean
  clearBackupPassword?: boolean
}

export type ActionApprovalSource = 'mcp' | 'ai-copilot'

/** One-time in-app approval shared by MCP and Copilot tool calls. */
export interface ActionApprovalRequest {
  requestId: string
  source: ActionApprovalSource
  operation: string
  title: string
  summary: string
  target?: string
  details?: string
  destructive: boolean
  /** Requires an explicit risk acknowledgement before the final approval. */
  requiresRiskAcknowledgement?: boolean
}

/** @deprecated Use ActionApprovalRequest. */
export type McpApprovalRequest = ActionApprovalRequest
export type SshInteractionDraft =
  | Omit<SshHostVerificationRequest, 'requestId' | 'tabId' | 'profileId'>
  | Omit<SshCredentialsPromptRequest, 'requestId' | 'tabId' | 'profileId'>
  | Omit<SshKeyPassphrasePromptRequest, 'requestId' | 'tabId' | 'profileId'>
  | Omit<SshKeyboardInteractiveRequest, 'requestId' | 'tabId' | 'profileId'>

export type SshHostVerificationResponse = {
  kind: 'host-verification'
  decision: 'accept-once' | 'accept-and-save' | 'cancel'
}

export type SshCredentialsPromptResponse =
  | {
      kind: 'credentials'
      canceled: true
    }
  | {
      kind: 'credentials'
      canceled: false
      username: string
      password: string
    }

export type SshKeyPassphrasePromptResponse =
  | {
      kind: 'key-passphrase'
      canceled: true
    }
  | {
      kind: 'key-passphrase'
      canceled: false
      passphrase: string
      savePassphrase: boolean
    }

export type SshKeyboardInteractiveResponse =
  | { kind: 'keyboard-interactive'; canceled: true }
  | { kind: 'keyboard-interactive'; canceled: false; answers: string[] }

export type SshInteractionResponse =
  | SshHostVerificationResponse
  | SshCredentialsPromptResponse
  | SshKeyPassphrasePromptResponse
  | SshKeyboardInteractiveResponse

export interface CommandTemplateInput {
  name: string
  command: string
  description?: string
  parentId?: string
  order?: number
  appendCarriageReturn?: boolean
}

export type ConnectionFormMode = 'create' | 'edit'

export type AppWindowMode = 'main' | 'connection-manager' | 'connection-form' | 'command-manager' | 'command-form'

export interface CommandExecutionResult {
  renderedCommand: string
}

export interface CommandExecutionOptions {
  appendCarriageReturn?: boolean
}

export interface TerminalCommandHistoryEntry {
  command: string
  createdAt: number
}

export type OverviewSectionId = 'stats' | 'recent' | 'allConnections' | 'quickActions'

export const DEFAULT_OVERVIEW_SECTION_ORDER = [
  'stats',
  'recent',
  'allConnections',
  'quickActions'
] as const satisfies readonly OverviewSectionId[]

export type ThemeVariant = 'dark' | 'light'

export type ThemeBaseId = 'fileterm' | 'codex'

export type TerminalAnsiColorName =
  | 'black'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'brightBlack'
  | 'brightRed'
  | 'brightGreen'
  | 'brightYellow'
  | 'brightBlue'
  | 'brightMagenta'
  | 'brightCyan'
  | 'brightWhite'

export type TerminalAnsiPalette = Record<TerminalAnsiColorName, string>

export interface TerminalThemeConfig {
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  selectionBackground: string
  selectionForeground: string
  ansi: TerminalAnsiPalette
  search: {
    matchBackground: string
    matchRuler: string
    activeMatchBackground: string
    activeMatchText: string
    activeMatchBorder: string
    activeMatchRuler: string
  }
}

export interface ThemeSemanticColors {
  diffAdded: string
  diffRemoved: string
  skill: string
  keyword: string
  total: string
  telnet: string
  ftp: string
  networkRx: string
  networkTx: string
  secondary: string
  textSecondary: string
  info: string
  warning: string
  error: string
  success: string
}

/**
 * Persist only the compact theme primitives and derive the larger UI alias
 * surface in the renderer. Advanced overrides are CSS custom properties, not
 * a persisted mirror of every component selector.
 */

export interface ThemeConfig {
  schemaVersion: 'codex-theme-v1'
  codeThemeId: string
  /** Built-in theme whose component skin this configuration inherits from. */
  baseThemeId?: ThemeBaseId
  variant: ThemeVariant
  theme: {
    accent: string
    contrast: number
    fonts: {
      code: string | null
      ui: string | null
    }
    ink: string
    opaqueWindows: boolean
    semanticColors: ThemeSemanticColors
    surface: string
    surfaceSecondary: string
    surfaceElevated: string
    /** Optional advanced color overrides; absent for normal themes. */
    overrides?: Record<string, string>
    terminal: TerminalThemeConfig
  }
}

export interface SavedTheme {
  id: string
  name: string
  config: ThemeConfig
  /** Independent dark/light variants for custom themes. */
  variants?: Partial<Record<ThemeVariant, ThemeConfig>>
}

const THEME_HEX_COLOR_PATTERN = /^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i

const DEFAULT_DARK_ANSI: TerminalAnsiPalette = {
  black: '#1c1d22',
  red: '#e05555',
  green: '#10b981',
  yellow: '#eab308',
  blue: '#3b82f6',
  magenta: '#c084fc',
  cyan: '#22d3ee',
  white: '#e2e8f0',
  brightBlack: '#64748b',
  brightRed: '#f87171',
  brightGreen: '#34d399',
  brightYellow: '#fde047',
  brightBlue: '#60a5fa',
  brightMagenta: '#e879f9',
  brightCyan: '#67e8f9',
  brightWhite: '#ffffff'
}

const DEFAULT_LIGHT_ANSI: TerminalAnsiPalette = {
  black: '#18181b',
  red: '#dc2626',
  green: '#16a34a',
  yellow: '#ca8a04',
  blue: '#2563eb',
  magenta: '#9333ea',
  cyan: '#0891b2',
  white: '#f1f5f9',
  brightBlack: '#64748b',
  brightRed: '#ef4444',
  brightGreen: '#22c55e',
  brightYellow: '#eab308',
  brightBlue: '#3b82f6',
  brightMagenta: '#a855f7',
  brightCyan: '#06b6d4',
  brightWhite: '#ffffff'
}

function defaultTerminalThemeConfig(variant: ThemeVariant): TerminalThemeConfig {
  const isLight = variant === 'light'
  return {
    background: isLight ? '#f4f4f6' : '#181818',
    foreground: isLight ? '#111827' : '#e0e0e0',
    cursor: isLight ? '#3b82f6' : '#ffffff',
    cursorAccent: isLight ? '#ffffff' : '#181818',
    selectionBackground: isLight ? '#0969DA42' : '#388BFD85',
    selectionForeground: isLight ? '#111827' : '#e0e0e0',
    ansi: { ...(isLight ? DEFAULT_LIGHT_ANSI : DEFAULT_DARK_ANSI) },
    search: {
      matchBackground: isLight ? '#f6cf57' : '#4b5563',
      matchRuler: isLight ? '#d39b16' : '#9ca3af',
      activeMatchBackground: '#ffd43b',
      activeMatchText: '#111111',
      activeMatchBorder: '#8a5a00',
      activeMatchRuler: '#f0b400'
    }
  }
}

export function createCodexThemeConfig(variant: ThemeVariant = 'dark'): ThemeConfig {
  const isLight = variant === 'light'
  return {
    schemaVersion: 'codex-theme-v1',
    codeThemeId: 'codex',
    baseThemeId: 'codex',
    variant,
    theme: {
      accent: isLight ? '#339cff' : '#0169cc',
      contrast: isLight ? 45 : 60,
      fonts: { code: null, ui: null },
      ink: isLight ? '#1a1c1f' : '#fcfcfc',
      opaqueWindows: false,
      semanticColors: {
        diffAdded: '#10b981',
        diffRemoved: isLight ? '#ba2623' : '#f43f5e',
        skill: isLight ? '#924ff7' : '#a855f7',
        keyword: isLight ? '#b45309' : '#fbbf24',
        total: isLight ? '#2563eb' : '#60a5fa',
        telnet: isLight ? '#0284c7' : '#38bdf8',
        ftp: isLight ? '#924ff7' : '#b06dff',
        networkRx: isLight ? '#0284c7' : '#38bdf8',
        networkTx: isLight ? '#ba2623' : '#f43f5e',
        secondary: isLight ? '#3b82f6' : '#8bbfff',
        textSecondary: isLight ? '#667085' : '#a1a1aa',
        info: isLight ? '#339cff' : '#38bdf8',
        warning: isLight ? '#b45309' : '#f59e0b',
        error: isLight ? '#ba2623' : '#f43f5e',
        success: isLight ? '#059669' : '#34d399'
      },
      surface: isLight ? '#ffffff' : '#111111',
      surfaceSecondary: isLight ? '#ffffff' : '#181818',
      surfaceElevated: isLight ? '#ffffff' : '#222222',
      terminal: {
        background: isLight ? '#ffffff' : '#111111',
        foreground: isLight ? '#1a1c1f' : '#fcfcfc',
        cursor: isLight ? '#339cff' : '#38bdf8',
        cursorAccent: isLight ? '#ffffff' : '#111111',
        selectionBackground: isLight ? '#339cff42' : '#38bdf844',
        selectionForeground: isLight ? '#1a1c1f' : '#fcfcfc',
        ansi: { ...(isLight ? DEFAULT_LIGHT_ANSI : DEFAULT_DARK_ANSI) },
        search: {
          matchBackground: isLight ? '#f6cf57' : '#4b5563',
          matchRuler: isLight ? '#d39b16' : '#9ca3af',
          activeMatchBackground: '#ffd43b',
          activeMatchText: '#111111',
          activeMatchBorder: '#8a5a00',
          activeMatchRuler: '#f0b400'
        }
      }
    }
  }
}

export function createDefaultThemeConfig(variant: ThemeVariant = 'dark'): ThemeConfig {
  const isLight = variant === 'light'
  return {
    schemaVersion: 'codex-theme-v1',
    codeThemeId: 'fileterm',
    baseThemeId: 'fileterm',
    variant,
    theme: {
      accent: isLight ? '#3b82f6' : '#1687e8',
      contrast: isLight ? 52 : 60,
      fonts: { code: null, ui: null },
      ink: isLight ? '#18181b' : '#e7e7e7',
      opaqueWindows: true,
      semanticColors: {
        diffAdded: isLight ? '#168a53' : '#34d399',
        diffRemoved: isLight ? '#d94e4e' : '#ff5f57',
        skill: isLight ? '#7c3aed' : '#b06dff',
        keyword: isLight ? '#b45309' : '#fbbf24',
        total: isLight ? '#2563eb' : '#60a5fa',
        telnet: isLight ? '#0284c7' : '#38bdf8',
        ftp: isLight ? '#9333ea' : '#c084fc',
        networkRx: isLight ? '#3b82f6' : '#65a9ff',
        networkTx: isLight ? '#ef4444' : '#ff7474',
        secondary: isLight ? '#3b82f6' : '#8bbfff',
        textSecondary: isLight ? '#5e5e61' : '#9b9b9b',
        info: isLight ? '#3b82f6' : '#38bdf8',
        warning: isLight ? '#d97706' : '#f59e0b',
        error: isLight ? '#d94e4e' : '#ff5f57',
        success: isLight ? '#168a53' : '#34d399'
      },
      surface: isLight ? '#F4F4F6' : '#151515',
      surfaceSecondary: isLight ? '#ffffff' : '#1e1e1e',
      surfaceElevated: isLight ? '#ffffff' : '#2a2a2a',
      terminal: defaultTerminalThemeConfig(variant)
    }
  }
}

function asThemeRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {}
}

function normalizeThemeColor(value: unknown, fallback: string) {
  return typeof value === 'string' && THEME_HEX_COLOR_PATTERN.test(value.trim()) ? value.trim().toUpperCase() : fallback
}

function normalizeThemeString(value: unknown, fallback: string | null) {
  if (value === null) return null
  return typeof value === 'string' && value.trim().length <= 256 ? value.trim() : fallback
}

function normalizeThemeFont(value: unknown, fallback: string | null) {
  if (value === null) return null
  if (typeof value !== 'string') return fallback
  const trimmed = value.trim()
  return /^[\w\u0080-\uffff][\w\u0080-\uffff .'-]{0,127}$/u.test(trimmed) ? trimmed : fallback
}

function collectThemeColorOverrides(value: unknown, prefix = '', result: Record<string, string> = {}) {
  if (typeof value === 'string') {
    if (prefix && THEME_HEX_COLOR_PATTERN.test(value.trim())) {
      result[prefix] = value.trim().toUpperCase()
    }
    return result
  }

  const record = asThemeRecord(value)
  for (const [key, child] of Object.entries(record)) {
    const nextPrefix = prefix ? `${prefix}.${key}` : key
    if (nextPrefix.length <= 128) {
      collectThemeColorOverrides(child, nextPrefix, result)
    }
  }
  return result
}

function normalizeThemeNumber(value: unknown, fallback: number) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback
  return Math.min(100, Math.max(0, Math.round(value)))
}

export function normalizeThemeConfig(value: unknown, fallbackVariant: ThemeVariant = 'dark'): ThemeConfig {
  const root = asThemeRecord(value)
  const rawCodeThemeId = typeof root.codeThemeId === 'string' ? root.codeThemeId.trim() : ''
  const isCodexCodeTheme = rawCodeThemeId === 'codex' || rawCodeThemeId.startsWith('codex-')
  const isFileTermTheme =
    rawCodeThemeId === 'fileterm' || rawCodeThemeId === 'fileterm-dark' || rawCodeThemeId === 'fileterm-light'
  const rawBaseThemeId = root.baseThemeId === 'codex' || root.baseThemeId === 'fileterm' ? root.baseThemeId : null
  const baseThemeId: ThemeBaseId = isCodexCodeTheme
    ? 'codex'
    : isFileTermTheme
      ? 'fileterm'
      : (rawBaseThemeId ?? 'fileterm')
  const isCodexTheme = baseThemeId === 'codex'
  const variant = root.variant === 'light' || root.variant === 'dark' ? root.variant : fallbackVariant
  const fallback = isCodexTheme ? createCodexThemeConfig(fallbackVariant) : createDefaultThemeConfig(fallbackVariant)
  const variantFallback = isCodexTheme
    ? createCodexThemeConfig(variant)
    : variant === fallbackVariant
      ? fallback
      : createDefaultThemeConfig(variant)
  const rawTheme = asThemeRecord(root.theme)
  const rawFonts = asThemeRecord(rawTheme.fonts)
  const rawSemanticColors = asThemeRecord(rawTheme.semanticColors)
  const rawTerminal = asThemeRecord(rawTheme.terminal)
  const rawAnsi = asThemeRecord(rawTerminal.ansi)
  const rawSearch = asThemeRecord(rawTerminal.search)
  const terminalFallback = variantFallback.theme.terminal
  const migrateLegacyCodexStatusColors =
    isCodexTheme &&
    Object.keys(asThemeRecord(rawTheme.overrides)).length === 0 &&
    typeof rawSemanticColors.sftp === 'string' &&
    rawSemanticColors.sftp.trim().toLowerCase() === '#0169cc' &&
    typeof rawSemanticColors.success === 'string' &&
    rawSemanticColors.success.trim().toLowerCase() === '#00a240'
  const normalizedSkill = normalizeThemeColor(rawSemanticColors.skill, variantFallback.theme.semanticColors.skill)
  const normalizedTotal = normalizeThemeColor(rawSemanticColors.total, variantFallback.theme.semanticColors.total)
  const rawTelnet = rawSemanticColors.telnet ?? rawSemanticColors.sftp
  const normalizedTelnet = migrateLegacyCodexStatusColors
    ? variantFallback.theme.semanticColors.telnet
    : normalizeThemeColor(rawTelnet, variantFallback.theme.semanticColors.telnet)
  const normalizedSuccess = migrateLegacyCodexStatusColors
    ? variantFallback.theme.semanticColors.success
    : normalizeThemeColor(rawSemanticColors.success, variantFallback.theme.semanticColors.success)
  const normalizedNetworkRx = normalizeThemeColor(
    rawSemanticColors.networkRx,
    variantFallback.theme.semanticColors.networkRx
  )
  const normalizedNetworkTx = normalizeThemeColor(
    rawSemanticColors.networkTx,
    variantFallback.theme.semanticColors.networkTx
  )
  const ftpFallback = isFileTermTheme ? variantFallback.theme.semanticColors.ftp : normalizedSkill
  const normalizedFtp = normalizeThemeColor(rawSemanticColors.ftp, ftpFallback)
  const normalizedSurface = normalizeThemeColor(rawTheme.surface, variantFallback.theme.surface)
  const normalizedSurfaceSecondary = normalizeThemeColor(
    rawTheme.surfaceSecondary,
    variantFallback.theme.surfaceSecondary
  )
  const normalizedSurfaceElevated = normalizeThemeColor(rawTheme.surfaceElevated, variantFallback.theme.surfaceElevated)
  const rawOverrides = Object.keys(asThemeRecord(rawTheme.overrides)).length
    ? rawTheme.overrides
    : isFileTermTheme
      ? undefined
      : rawTheme.ui
  const overrides = collectThemeColorOverrides(rawOverrides)

  const ansi = Object.fromEntries(
    (Object.keys(terminalFallback.ansi) as TerminalAnsiColorName[]).map((name) => [
      name,
      normalizeThemeColor(rawAnsi[name], terminalFallback.ansi[name])
    ])
  ) as TerminalAnsiPalette

  return {
    schemaVersion: 'codex-theme-v1',
    codeThemeId: isFileTermTheme
      ? 'fileterm'
      : rawCodeThemeId === 'codex-dark' || rawCodeThemeId === 'codex-light'
        ? 'codex'
        : (normalizeThemeString(root.codeThemeId, variantFallback.codeThemeId) ?? variantFallback.codeThemeId),
    baseThemeId,
    variant,
    theme: {
      accent: normalizeThemeColor(rawTheme.accent, variantFallback.theme.accent),
      contrast: normalizeThemeNumber(rawTheme.contrast, variantFallback.theme.contrast),
      fonts: {
        code: normalizeThemeFont(rawFonts.code, variantFallback.theme.fonts.code),
        ui: normalizeThemeFont(rawFonts.ui, variantFallback.theme.fonts.ui)
      },
      ink: normalizeThemeColor(rawTheme.ink, variantFallback.theme.ink),
      opaqueWindows:
        typeof rawTheme.opaqueWindows === 'boolean' ? rawTheme.opaqueWindows : variantFallback.theme.opaqueWindows,
      semanticColors: {
        diffAdded: normalizeThemeColor(rawSemanticColors.diffAdded, variantFallback.theme.semanticColors.diffAdded),
        diffRemoved: normalizeThemeColor(
          rawSemanticColors.diffRemoved,
          variantFallback.theme.semanticColors.diffRemoved
        ),
        skill: normalizedSkill,
        keyword: normalizeThemeColor(rawSemanticColors.keyword, variantFallback.theme.semanticColors.keyword),
        total: normalizedTotal,
        telnet: normalizedTelnet,
        ftp: normalizedFtp,
        networkRx: normalizedNetworkRx,
        networkTx: normalizedNetworkTx,
        secondary: normalizeThemeColor(rawSemanticColors.secondary, variantFallback.theme.semanticColors.secondary),
        textSecondary: normalizeThemeColor(
          rawSemanticColors.textSecondary,
          variantFallback.theme.semanticColors.textSecondary
        ),
        info: normalizeThemeColor(rawSemanticColors.info, variantFallback.theme.semanticColors.info),
        warning: normalizeThemeColor(rawSemanticColors.warning, variantFallback.theme.semanticColors.warning),
        error: normalizeThemeColor(rawSemanticColors.error, variantFallback.theme.semanticColors.error),
        success: normalizedSuccess
      },
      surface: normalizedSurface,
      surfaceSecondary: normalizedSurfaceSecondary,
      surfaceElevated: normalizedSurfaceElevated,
      ...(Object.keys(overrides).length > 0 ? { overrides } : {}),
      terminal: {
        background: normalizeThemeColor(rawTerminal.background, terminalFallback.background),
        foreground: normalizeThemeColor(rawTerminal.foreground, terminalFallback.foreground),
        cursor: normalizeThemeColor(rawTerminal.cursor, terminalFallback.cursor),
        cursorAccent: normalizeThemeColor(rawTerminal.cursorAccent, terminalFallback.cursorAccent),
        selectionBackground: normalizeThemeColor(rawTerminal.selectionBackground, terminalFallback.selectionBackground),
        selectionForeground: normalizeThemeColor(rawTerminal.selectionForeground, terminalFallback.selectionForeground),
        ansi,
        search: {
          matchBackground: normalizeThemeColor(rawSearch.matchBackground, terminalFallback.search.matchBackground),
          matchRuler: normalizeThemeColor(rawSearch.matchRuler, terminalFallback.search.matchRuler),
          activeMatchBackground: normalizeThemeColor(
            rawSearch.activeMatchBackground,
            terminalFallback.search.activeMatchBackground
          ),
          activeMatchText: normalizeThemeColor(rawSearch.activeMatchText, terminalFallback.search.activeMatchText),
          activeMatchBorder: normalizeThemeColor(
            rawSearch.activeMatchBorder,
            terminalFallback.search.activeMatchBorder
          ),
          activeMatchRuler: normalizeThemeColor(rawSearch.activeMatchRuler, terminalFallback.search.activeMatchRuler)
        }
      }
    }
  }
}

export interface UiPreferences {
  theme: 'default-dark' | 'default-light'
  locale: 'zhCN' | 'enUS'
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  autoCheckUpdates: boolean
  updateChannel: AppUpdateChannel
  terminalZoomLocked: boolean
  localTerminalShells: LocalTerminalShellPreferences
  filePanelRememberRatio: boolean
  resourceMonitoringMetrics: ResourceMonitoringMetric[]
  resourceMonitoringMetricOrder: ResourceMonitoringMetric[]
  connectionDefaults: SshConnectionDefaults
  mcpAgent: McpAgentPreferences
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
}

export interface UiPreferencesInput {
  theme?: UiPreferences['theme']
  locale?: UiPreferences['locale']
  themeConfig?: ThemeConfig
  customThemes?: SavedTheme[]
  autoCheckUpdates?: boolean
  updateChannel?: UiPreferences['updateChannel']
  terminalZoomLocked?: boolean
  localTerminalShells?: Partial<LocalTerminalShellPreferences>
  filePanelRememberRatio?: boolean
  resourceMonitoringMetrics?: ResourceMonitoringMetric[]
  resourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  connectionDefaults?: Partial<SshConnectionDefaults>
  mcpAgent?: Partial<McpAgentPreferences>
  overviewShowStats?: boolean
  overviewShowRecent?: boolean
  overviewShowAllConnections?: boolean
  overviewShowQuickActions?: boolean
  overviewSectionOrder?: OverviewSectionId[]
}

export interface CommandSendPreferences {
  rememberSelection: boolean
  sendScope: 'current' | 'all-ssh' | 'selected-ssh'
  selectedTabIds: string[]
}

export interface TerminalDataPayload {
  tabId: string
  chunk: string
}

export interface TerminalStatePayload {
  tabId: string
  summary: string
  transcript: string
  connected: boolean
  status: TabStatus
}

export interface RemoteFileAccessOptions {
  rootAccessMethod?: 'sudo' | 'su'
  sudoUser?: string
  sudoPassword?: string
  /** Ask the backend to select the matching saved sudo/su credential. */
  useSavedPassword?: boolean
}

export type AiProviderKind = 'openai-compatible-chat' | 'openai-responses' | 'anthropic-messages'

export interface AiProviderSummary {
  id: string
  name: string
  kind: AiProviderKind
  baseUrl: string
  model: string
  models?: string[]
  enabled: boolean
  hasApiKey: boolean
  usable: boolean
  isDefault: boolean
  allowNoAuth: boolean
  allowInsecureHttp: boolean
}

export interface AiProviderDraft {
  id?: string
  name: string
  kind: AiProviderKind
  baseUrl: string
  model: string
  models?: string[]
  enabled: boolean
  isDefault: boolean
  allowNoAuth: boolean
  allowInsecureHttp: boolean
}

export interface AiProviderSecretPatch {
  /**
   * An omitted value preserves the saved key; a non-empty string replaces it;
   * null removes it. The API never returns the plaintext key.
   */
  apiKey?: string | null
}

export interface SaveAiProviderInput {
  provider: AiProviderDraft
  secrets?: AiProviderSecretPatch
}

export interface TestAiProviderInput {
  provider: AiProviderDraft
  secrets?: AiProviderSecretPatch
}

export interface AiProviderTestResult {
  ok: true
  message: string
}

/** The user-visible Copilot execution mode. */
export type AiCopilotMode = 'pure-conversation' | 'semi-automatic' | 'fully-automatic'

/** The only context levels exposed by the new Copilot contract. */
export type AiContextLevel = 'L0' | 'L2'

/**
 * Context values accepted while old conversations and renderers migrate.
 * Rust normalizes `metadata` to L2 instead of preserving the old L1 meaning.
 */
export type AiContextMode = AiContextLevel | 'metadata' | 'recent-terminal'

export interface AiAutoModeGuardrailState {
  dangerousCommandRestrictionsEnabled: boolean
}

export interface AiCopilotModeState {
  mode: AiCopilotMode
  /** Only pure-conversation mode can change this flag. */
  attachTerminalContext: boolean
  autoModeGuardrails: AiAutoModeGuardrailState
}

export interface SetAiCopilotModeInput {
  mode: AiCopilotMode
  /** Required only when entering fully-automatic mode. */
  confirmed?: boolean
}

export interface SetAiContextAttachInput {
  attachTerminalContext: boolean
}

export interface SetAiDangerousCommandRestrictionsInput {
  enabled: boolean
}

/** A target identity that a one-time context snapshot is bound to. */
export interface AiContextTarget {
  tabId: string
  rootTabId: string
  sessionType: 'ssh' | 'local'
  /** Changes when the interactive target changes, reconnects, or its shell identity/CWD changes. */
  sessionRevision: string
  displayHost: string
  user?: string
  cwd?: string
  connected: boolean
}

/** A best-effort sanitization applied before terminal text can leave the device. */
export interface AiContextRedaction {
  kind: 'authorization' | 'credential-assignment' | 'private-key' | 'control-sequence' | 'long-line'
  count: number
}

/** Exact, immutable context that the user reviews before one request can consume it. */
export interface AiContextPreview {
  snapshotId: string
  expiresAt: string
  mode: AiContextMode
  target: AiContextTarget
  preview: string
  redactions: AiContextRedaction[]
  truncated: boolean
}

/** Non-sensitive audit metadata retained with a local user message; raw terminal text is never stored. */
export interface AiContextAttachment {
  mode: AiContextMode
  target: AiContextTarget
  redactions: AiContextRedaction[]
  truncated: boolean
}

export type AiCommandRisk = 'read-only' | 'mutating' | 'destructive' | 'privileged' | 'unknown'

export type AiToolCallStatus =
  | 'approved'
  | 'rejected'
  | 'auto-blocked'
  | 'executed'
  | 'executed-in-terminal'
  | 'failed'
  | 'timeout'
  | 'target-changed'
  | 'input-required'
  | 'invalid'

/** A persisted Copilot tool result, including migrated command/review history. */
export interface AiToolCallResult {
  proposalId: string
  status: AiToolCallStatus
  exitCode?: number
  stdout?: string
  stderr?: string
  durationMs?: number
  reason?: string
  recordId?: string
  requestedAt?: string
  approvedAt?: string
  completedAt?: string
  timeoutMs?: number
  outputTruncated?: boolean
}

export interface AiToolCallProposal {
  id: string
  toolName: 'fileterm_execute_remote_command'
  command: string
  risk: AiCommandRisk
  target: AiContextTarget
  explanation?: string
  /** Links a semi-automatic tool call to its inline approval card. */
  approvalRequestId?: string
}

export interface AiToolActivity {
  proposal: AiToolCallProposal
  result?: AiToolCallResult
}

/** A message stored locally for an AI Copilot conversation. */
export interface AiMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  createdAt: string
  /** Present only for an explicitly approved L2 user turn; never contains raw transcript text. */
  context?: AiContextAttachment
  /** Persisted tool proposals/results rendered inline with the assistant turn. */
  toolActivities?: AiToolActivity[]
}

/** Lightweight metadata used by the Copilot conversation switcher. */
export interface AiConversationSummary {
  id: string
  title: string
  providerId: string
  createdAt: string
  updatedAt: string
  messageCount: number
}

/** A complete local AI Copilot conversation. */
export interface AiConversation extends AiConversationSummary {
  messages: AiMessage[]
}

export interface CreateAiConversationInput {
  providerId: string
}

export interface RenameAiConversationInput {
  conversationId: string
  title: string
}

export interface DeleteAiMessageInput {
  conversationId: string
  messageId: string
}

/** Requests automatic title generation from the configured Provider. */
export interface SummarizeAiConversationTitleInput {
  conversationId: string
  providerId: string
  modelOverride?: string
}

export interface CreateAiContextPreviewInput {
  tabId: string
  /** Optional renderer hint. Rust resolves and validates the actual root relation. */
  rootTabId?: string
  providerId: string
  mode: AiContextLevel
}

/**
 * A context ID can only refer to a Rust-owned, reviewed one-time snapshot.
 * The renderer cannot supply terminal text, host data, or command text here.
 */
export interface StartAiChatInput {
  conversationId: string
  providerId: string
  modelOverride?: string
  userMessage: string
  contextSnapshotId?: string
  /** Optional for callers that need to pin the active mode. */
  mode?: AiCopilotMode
}

/** Retries the latest user turn without duplicating it in local history. */
export interface RetryAiChatInput {
  conversationId: string
  providerId: string
  modelOverride?: string
  contextSnapshotId?: string
  mode?: AiCopilotMode
}

export interface AiChatRequest {
  requestId: string
  conversationId: string
  userMessageId: string
  assistantMessageId: string
}

/** Per-request stream events; never emitted through a global application event. */
export type AiStreamEvent =
  | { type: 'started'; requestId: string; messageId: string }
  | { type: 'assistant-message-started'; messageId: string }
  | { type: 'text-delta'; text: string }
  | { type: 'tool-call'; proposal: AiToolCallProposal }
  | { type: 'tool-result'; result: AiToolCallResult }
  | { type: 'usage'; inputTokens?: number; outputTokens?: number }
  | { type: 'completed'; conversation: AiConversation; finishReason?: string }
  | { type: 'error'; code: AiErrorCode; message: string; retryable: boolean }

export type AiErrorCode =
  | 'AI_PROVIDER_NOT_FOUND'
  | 'AI_PROVIDER_INVALID_CONFIG'
  | 'AI_PROVIDER_INVALID_URL'
  | 'AI_PROVIDER_INSECURE_HTTP'
  | 'AI_PROVIDER_AUTH_REQUIRED'
  | 'AI_PROVIDER_CONNECTION_FAILED'
  | 'AI_PROVIDER_HTTP_ERROR'
  | 'AI_PROVIDER_RESPONSE_INVALID'
  | 'AI_PROVIDER_TIMEOUT'
  | 'AI_REQUEST_CANCELLED'
  | 'AI_CONTEXT_NOT_FOUND'
  | 'AI_CONTEXT_EXPIRED'
  | 'AI_CONTEXT_ALREADY_USED'
  | 'AI_CONTEXT_TARGET_CHANGED'
  | 'AI_CONTEXT_FORBIDDEN'
  | 'AI_COMMAND_NOT_FOUND'
  | 'AI_COMMAND_UNSAFE_INPUT'
  | 'AI_REVIEW_IN_PROGRESS'
  | 'AI_REVIEW_UNAVAILABLE'
  | 'AI_MODE_CONFIRMATION_REQUIRED'
  | 'AI_MODE_CHANGED'
  | 'AI_CONTEXT_LOCKED'
  | 'AI_AUTO_MODE_BLOCKED_COMMAND'
  | 'AI_AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED'
  | 'AI_AUTO_MODE_TARGET_CHANGED'
  | 'AI_TOOL_CALL_REJECTED'
  | 'AI_TOOL_CALL_INVALID'
  | 'AI_TOOL_LOOP_LIMIT'
  | 'AI_CONVERSATION_LIMIT'
  | 'AI_CONVERSATION_ACTIVE'
  | 'AI_CONVERSATION_NOT_FOUND'
  | 'AI_MESSAGE_NOT_FOUND'
  | 'AI_CONVERSATION_INVALID_INPUT'

export interface AiCommandError {
  code: AiErrorCode
  message: string
  retryable: boolean
  httpStatus?: number
}

export type SerialPortType = 'usb' | 'pci' | 'bluetooth' | 'unknown'

export interface SerialPortInfo {
  portName: string
  portType: SerialPortType
  manufacturer?: string
  product?: string
  serialNumber?: string
  vendorId?: number
  productId?: number
}

export interface FileTermDesktopApi {
  platform: string
  arch: string
  appVersion: string
  appName: string
  runtimeName: 'Electron' | 'Tauri'
  runtimeVersion: string
  isDesktop: boolean
  getUpdateStatus(): Promise<AppUpdateStatus>
  checkForUpdates(): Promise<AppUpdateStatus>
  downloadUpdate(): Promise<void>
  installUpdate(): Promise<void>
  onUpdateStatus(listener: (status: AppUpdateStatus) => void): () => void
  readClipboardText(): Promise<string>
  writeClipboardText(text: string): Promise<void>
  getUiPreferences(): Promise<UiPreferences>
  setUiPreferences(input: UiPreferencesInput): Promise<UiPreferences>
  getSecuritySettings(): Promise<SecuritySettings>
  setSecuritySettings(input: SecuritySettingsInput): Promise<SecuritySettings>
  verifySecurityPassword(password: string): Promise<boolean>
  resetSecurityBackupPassword(): Promise<SecuritySettings>
  listLocalTerminalShells(): Promise<LocalTerminalShellOption[]>
  getMcpAgentSetup(): Promise<McpAgentSetup>
  listAiProviders(): Promise<AiProviderSummary[]>
  saveAiProvider(input: SaveAiProviderInput): Promise<AiProviderSummary>
  deleteAiProvider(providerId: string): Promise<AiProviderSummary[]>
  testAiProvider(input: TestAiProviderInput): Promise<AiProviderTestResult>
  listAiConversations(): Promise<AiConversationSummary[]>
  getAiConversation(conversationId: string): Promise<AiConversation>
  createAiConversation(input: CreateAiConversationInput): Promise<AiConversation>
  renameAiConversation(input: RenameAiConversationInput): Promise<AiConversation>
  summarizeAiConversationTitle(input: SummarizeAiConversationTitleInput): Promise<AiConversation>
  deleteAiMessage(input: DeleteAiMessageInput): Promise<AiConversation>
  deleteAiConversation(conversationId: string): Promise<void>
  getAiCopilotModeState(): Promise<AiCopilotModeState>
  setAiCopilotMode(input: SetAiCopilotModeInput): Promise<AiCopilotModeState>
  setAiContextAttach(input: SetAiContextAttachInput): Promise<AiCopilotModeState>
  setAiDangerousCommandRestrictions(input: SetAiDangerousCommandRestrictionsInput): Promise<AiCopilotModeState>
  createAiContextPreview(input: CreateAiContextPreviewInput): Promise<AiContextPreview>
  startAiChat(input: StartAiChatInput, onEvent: (event: AiStreamEvent) => void): Promise<AiChatRequest>
  retryAiChat(input: RetryAiChatInput, onEvent: (event: AiStreamEvent) => void): Promise<AiChatRequest>
  cancelAiChat(requestId: string): Promise<void>
  getUiStateItem(key: string): Promise<string | null>
  setUiStateItem(key: string, value: string): Promise<void>
  removeUiStateItem(key: string): Promise<void>
  openConnectionManagerWindow(): Promise<void>
  openCommandManagerWindow(): Promise<void>
  openConnectionFormWindow(mode: ConnectionFormMode, profileId?: string): Promise<void>
  openCommandFormWindow(
    mode: ConnectionFormMode,
    commandId?: string,
    folderId?: string,
    command?: string
  ): Promise<void>
  openFileEditorWindow(input: FileEditorWindowInput): Promise<void>
  openExternalUrl(url: string): Promise<void>
  openLogsDirectory(): Promise<void>
  listSerialPorts(): Promise<SerialPortInfo[]>
  onSerialPortsChanged(listener: (ports: SerialPortInfo[]) => void): () => void
  serialControl(
    tabId: string,
    action: SerialControlAction,
    value?: boolean,
    durationMs?: number
  ): Promise<SerialLineStatus>
  serialTransfer(
    tabId: string,
    direction: SerialTransferDirection,
    mode: SerialTransferMode,
    localPath: string,
    fileName?: string,
    localPaths?: string[],
    xmodemPreservePadding?: boolean
  ): Promise<SerialTransferResult>
  serialTransferCancel(tabId: string): Promise<void>
  saveSessionLog(tabId: string): Promise<string | null>
  minimizeCurrentWindow(): Promise<void>
  showCurrentWindow(): Promise<void>
  isCurrentWindowMaximized(): Promise<boolean>
  toggleMaximizeCurrentWindow(): Promise<void>
  closeCurrentWindow(): Promise<void>
  confirmCloseCurrentFileEditor(): Promise<void>
  cancelCloseCurrentFileEditor(): Promise<void>
  showWindowMenu(menuType: 'app' | 'file' | 'view' | 'window', x: number, y: number): Promise<void>
  reloadCurrentWindow(): Promise<void>
  toggleDevtools(): Promise<void>
  requestCloseCurrentWindow(): Promise<void>
  onWindowMaximizedChange(listener: (isMaximized: boolean) => void): () => void
  onUiPreferencesChanged(listener: (preferences: UiPreferences) => void): () => void
  onSecuritySettingsChanged(listener: (settings: SecuritySettings) => void): () => void
  onFileEditorCloseRequest(listener: () => void): () => void
  requestQuitApp(): Promise<void>
  getSnapshot(): Promise<WorkspaceSnapshot>
  getConnectionLibrary(): Promise<ConnectionLibrarySnapshot>
  listImportedFonts(): Promise<ImportedFont[]>
  importFont(): Promise<ImportedFont | null>
  getImportedFontData(fontId: string): Promise<string | null>
  deleteImportedFont(fontId: string): Promise<boolean>
  listSshKeys(): Promise<SshKeyMetadata[]>
  selectSshKeyFile(): Promise<SshKeyFileSelection | null>
  importSshKey(input?: ImportSshKeyInput): Promise<SshKeyImportResult | null>
  updateSshKeyNote(keyId: string, note: string): Promise<SshKeyMetadata>
  deleteSshKey(keyId: string): Promise<void>
  onSshKeysChanged(listener: (keys: SshKeyMetadata[]) => void): () => void
  previewConnectionImport(source?: 'files' | 'folder'): Promise<ConnectionImportPlan | null>
  commitConnectionJsonImport(planId: string, options: ConnectionImportOptions): Promise<ConnectionImportResult>
  exportConnections(format: ConnectionExportFormat): Promise<boolean>
  exportConnectionsAsFiles(format: ConnectionExportFormat): Promise<boolean>
  listSshTunnels(tabId: string): Promise<SshTunnelSnapshot[]>
  createSshTunnel(tabId: string, rule: SshForwardRule): Promise<SshTunnelSnapshot[]>
  startSshTunnel(tabId: string, ruleId: string): Promise<SshTunnelSnapshot[]>
  stopSshTunnel(tabId: string, ruleId: string): Promise<SshTunnelSnapshot[]>
  deleteSshTunnel(tabId: string, ruleId: string): Promise<SshTunnelSnapshot[]>
  getWebDavSyncConfig(): Promise<WebDavSyncConfig>
  saveWebDavSyncConfig(input: WebDavSyncConfig & { password?: string }): Promise<WebDavSyncConfig>
  testWebDavSync(): Promise<WebDavSyncResult>
  uploadWebDavSync(mode: BackupUploadMode): Promise<WebDavSyncResult>
  downloadWebDavSync(mode: BackupDownloadMode): Promise<WebDavSyncResult>
  getS3BackupConfig(): Promise<S3BackupConfig>
  saveS3BackupConfig(input: S3BackupConfigInput): Promise<S3BackupConfig>
  testS3Backup(): Promise<S3BackupResult>
  uploadS3Backup(mode: BackupUploadMode): Promise<S3BackupResult>
  downloadS3Backup(mode: BackupDownloadMode): Promise<S3BackupResult>
  createFolder(name: string, parentId?: string): Promise<WorkspaceSnapshot>
  updateFolder(folderId: string, updates: Partial<ConnectionFolder>): Promise<WorkspaceSnapshot>
  deleteFolder(folderId: string): Promise<WorkspaceSnapshot>
  updateEntityOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot>
  createCommandFolder(name: string, parentId?: string): Promise<WorkspaceSnapshot>
  updateCommandFolder(folderId: string, updates: Partial<CommandFolder>): Promise<WorkspaceSnapshot>
  deleteCommandFolder(folderId: string): Promise<WorkspaceSnapshot>
  updateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot>
  createCommandTemplate(input: CommandTemplateInput): Promise<WorkspaceSnapshot>
  updateCommandTemplate(commandId: string, input: CommandTemplateInput): Promise<WorkspaceSnapshot>
  deleteCommandTemplate(commandId: string): Promise<WorkspaceSnapshot>
  executeCommandTemplate(
    tabId: string,
    commandId: string,
    args?: string[],
    options?: CommandExecutionOptions
  ): Promise<CommandExecutionResult>
  executeRemoteCommand(
    tabId: string,
    command: string,
    cwd?: string,
    timeoutMs?: number,
    credentials?: RemoteExecCredentials
  ): Promise<{
    output: string
    exitCode: number | null
    timedOut: boolean
    outputTruncated: boolean
    /** The non-interactive channel saw a supported input prompt. */
    inputRequired: boolean
    /** A bounded routing hint; the input itself is never returned. */
    inputKind?: 'secret' | 'text'
  }>
  getTerminalCommandHistory(profileId: string): Promise<TerminalCommandHistoryEntry[]>
  setTerminalCommandHistory(profileId: string, entries: TerminalCommandHistoryEntry[]): Promise<void>
  getCommandSendPreferences(): Promise<CommandSendPreferences>
  setCommandSendPreferences(preferences: CommandSendPreferences): Promise<void>
  createProfile(input: CreateProfileInput): Promise<WorkspaceSnapshot>
  updateProfile(profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot>
  clearTrustedHostFingerprint(profileId: string): Promise<WorkspaceSnapshot>
  testConnection(input: CreateProfileInput, profileId?: string): Promise<void>
  deleteProfile(profileId: string): Promise<WorkspaceSnapshot>
  openProfile(profileId: string): Promise<WorkspaceSnapshot>
  openProfileFromManager(profileId: string): Promise<WorkspaceSnapshot>
  activateTab(tabId: string): Promise<WorkspaceSnapshot>
  reconnectTab(tabId: string): Promise<WorkspaceSnapshot>
  disconnectTab(tabId: string): Promise<WorkspaceSnapshot>
  closeTab(tabId: string): Promise<WorkspaceSnapshot>
  splitTab(sourceTabId: string, direction: 'row' | 'column'): Promise<WorkspaceSnapshot>
  closePane(rootTabId: string, paneTabId: string): Promise<WorkspaceSnapshot>
  setActivePane(rootTabId: string, paneTabId: string): Promise<WorkspaceSnapshot>
  setPaneWeights(rootTabId: string, panePath: number[], weights: number[]): Promise<WorkspaceSnapshot>
  listLocalDirectory(dirPath?: string): Promise<DirectorySnapshot<LocalFileItem>>
  connectLocalNetworkShare?(
    path: string,
    username: string,
    password: string,
    share?: string
  ): Promise<LocalNetworkShareConnectionResult>
  readLocalFile(filePath: string, encoding?: string): Promise<string>
  writeLocalFile(filePath: string, content: string, encoding?: string): Promise<void>
  createLocalDirectory(dirPath: string, name: string): Promise<void>
  createLocalFile(dirPath: string, name: string): Promise<void>
  copyLocalPath(sourcePath: string, destinationPath: string): Promise<void>
  moveLocalPath(sourcePath: string, destinationPath: string): Promise<void>
  renameLocalPath(targetPath: string, newName: string): Promise<void>
  deleteLocalPath(targetPath: string): Promise<void>
  changeLocalPermissions(targetPath: string, options: PermissionChangeOptions): Promise<void>
  getDroppedFilePaths(files: File[]): string[]
  selectLocalFiles(defaultPath?: string): Promise<string[]>
  selectLocalDirectory(defaultPath?: string): Promise<string | null>
  queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot>
  cancelTransfer(transferId: string): Promise<WorkspaceSnapshot>
  pauseTransfer(transferId: string): Promise<WorkspaceSnapshot>
  resumeTransfer(transferId: string): Promise<WorkspaceSnapshot>
  discardTransfer(transferId: string): Promise<WorkspaceSnapshot>
  clearTransfers(transferIds: string[]): Promise<WorkspaceSnapshot>
  uploadFile(
    tabId: string,
    localPath: string,
    remoteDirectory: string,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot>
  downloadFile(
    tabId: string,
    remotePath: string,
    localDirectory: string,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot>
  downloadRemotePath(
    tabId: string,
    remotePath: string,
    targetType: RemoteFileItem['type'],
    localDirectory: string,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot>
  setRemoteFileAccessMode(
    tabId: string,
    mode: 'user' | 'root',
    options?: RemoteFileAccessOptions
  ): Promise<WorkspaceSnapshot>
  openLocalTerminal(options?: LocalTerminalLaunchOptions): Promise<WorkspaceSnapshot>
  writeTerminal(tabId: string, data: string): Promise<void>
  resizeTerminal(tabId: string, cols: number, rows: number, width: number, height: number): Promise<void>
  openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot>
  setFollowShellCwd(tabId: string, enabled: boolean): Promise<WorkspaceSnapshot>
  readRemoteFile(tabId: string, targetPath: string, encoding?: string): Promise<string>
  writeRemoteFile(tabId: string, targetPath: string, content: string, encoding?: string): Promise<WorkspaceSnapshot>
  createRemoteDirectory(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot>
  createRemoteFile(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot>
  copyRemotePath(
    tabId: string,
    targetPath: string,
    destinationPath: string,
    targetType: RemoteFileItem['type']
  ): Promise<WorkspaceSnapshot>
  moveRemotePath(tabId: string, targetPath: string, destinationPath: string): Promise<WorkspaceSnapshot>
  renameRemotePath(tabId: string, targetPath: string, newName: string): Promise<WorkspaceSnapshot>
  deleteRemotePath(
    tabId: string,
    targetPath: string,
    targetType: RemoteFileItem['type'],
    isSymlink?: boolean
  ): Promise<WorkspaceSnapshot>
  resolveSshInteraction(requestId: string, response: SshInteractionResponse): Promise<void>
  resolveSudoPasswordPrompt(requestId: string, cancelled: boolean, value?: string, save?: boolean): Promise<void>
  setSudoPasswordPromptRendererReady(registrationId: string, ready: boolean): Promise<void>
  resolveBackupPassword(requestId: string, cancelled: boolean, value?: string): Promise<void>
  setBackupPasswordRendererReady(registrationId: string, ready: boolean): Promise<void>
  resolveActionApproval(requestId: string, approved: boolean): Promise<void>
  /** Hand a pending Copilot command to the already-visible terminal. */
  resolveAiTerminalHandoff(requestId: string): Promise<void>
  /** @deprecated Use resolveActionApproval. */
  resolveMcpApproval(requestId: string, approved: boolean): Promise<void>
  changeRemotePermissions(
    tabId: string,
    targetPath: string,
    options: PermissionChangeOptions
  ): Promise<WorkspaceSnapshot>
  onTerminalData(listener: (payload: TerminalDataPayload) => void): () => void
  onTerminalState(listener: (payload: TerminalStatePayload) => void): () => void
  onTransferUpdate(listener: (transfer: TransferTask) => void): () => void
  onSerialTransferProgress(listener: (progress: SerialTransferProgress) => void): () => void
  onWorkspaceSnapshot(listener: (snapshot: WorkspaceSnapshot) => void): () => void
  onSessionMetrics(listener: (payload: SessionMetricsUpdate) => void): () => void
  /** Resolves only after the SSH interaction listener is registered in Tauri. */
  onSshInteraction(listener: (request: SshInteractionRequest) => void): Promise<() => void>
  onSudoPasswordPrompt(listener: (request: SudoPasswordRequest) => void): Promise<() => void>
  /** Resolves only after the main renderer has registered the password prompt listener. */
  onBackupPasswordRequest(listener: (request: BackupPasswordRequest) => void): Promise<() => void>
  onActionApprovalRequest(listener: (request: ActionApprovalRequest) => void): () => void
  /** @deprecated Use onActionApprovalRequest. */
  onMcpApprovalRequest(listener: (request: McpApprovalRequest) => void): () => void
  onWindowCloseRequest(listener: (event: { isQuit: boolean }) => void): () => void
  onRequestCloseActiveWorkspaceItem(listener: () => void): () => void
  onNewTabRequest(listener: () => void): () => void
  onSplitPaneRequest(listener: (direction: 'row' | 'column') => void): () => void
  onFocusPaneRequest(listener: (direction: PaneFocusDirection) => void): () => void
  onTerminalZoomRequest(listener: (operation: TerminalZoomOperation) => void): () => void
  onTerminalGestureZoomRequest(listener: (operation: TerminalZoomOperation) => void): () => void
  confirmCloseWindow(action: 'quit' | 'hide' | 'cancel'): Promise<void>
}

export type AppUpdateState =
  'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'not-available' | 'error' | 'unsupported'

export type AppUpdateMode = 'in-app' | 'release-page'

export type AppUpdateChannel = 'stable' | 'beta'

export interface AppUpdateStatus {
  state: AppUpdateState
  currentVersion: string
  updateMode?: AppUpdateMode
  updateChannel?: AppUpdateChannel
  availableVersion?: string
  releaseTag?: string
  releaseUrl?: string
  progress?: number
  message?: string
}

export interface SessionController {
  readonly id: string
  readonly type: SessionType
  connect(): Promise<void>
  disconnect(): Promise<void>
  getSummary(): string
}

export interface TerminalSessionController extends SessionController {
  readonly type: 'ssh' | 'telnet' | 'serial'
  getTerminalTranscript(): string
  write(data: string): Promise<void>
  resize(cols: number, rows: number, width: number, height: number): Promise<void>
}

export interface ShellSessionController extends TerminalSessionController {
  readonly type: 'ssh'
  getShellCwd(): string | undefined
}

export interface FileSessionController extends SessionController {
  getRemotePath(): string
  getFileAccessMode(): 'user' | 'root'
  hasReusableSudoAuth(): boolean
  setFileAccessMode(mode: 'user' | 'root', options?: RemoteFileAccessOptions): Promise<void>
  listRemoteFiles(): Promise<RemoteFileItem[]>
  openRemotePath(path: string): Promise<RemoteFileItem[]>
  readRemoteFile(path: string, encoding?: string): Promise<string>
  writeRemoteFile(path: string, content: string, encoding?: string): Promise<void>
  copyRemotePath(path: string, destinationPath: string, targetType: RemoteFileItem['type']): Promise<void>
  moveRemotePath(path: string, destinationPath: string): Promise<void>
  renameRemotePath(path: string, nextPath: string): Promise<void>
  deleteRemotePath(path: string, targetType: RemoteFileItem['type']): Promise<void>
  changeRemotePermissions(path: string, options: PermissionChangeOptions): Promise<void>
  ensureRemoteDirectory(path: string): Promise<void>
  abortTransfer(): Promise<void>
  statRemoteFile(path: string): Promise<RemoteFileStat | null>
  replaceRemoteFile(partialPath: string, destinationPath: string): Promise<void>
  removeRemoteFileIfExists(path: string): Promise<void>
  uploadFile(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    options?: TransferFileOptions
  ): Promise<void>
  downloadFile(
    remotePath: string,
    localPath: string,
    onProgress: (progress: TransferProgress) => void,
    options?: TransferFileOptions
  ): Promise<void>
}

export interface SshSessionController extends ShellSessionController, FileSessionController {
  readonly type: 'ssh'
}

export interface FtpSessionController extends FileSessionController {
  readonly type: 'ftp'
}

export const createTabLayout = (profile: ConnectionProfile): TabLayout => {
  return profile.type === 'ssh' ? 'terminal-file' : profile.type === 'ftp' ? 'file-only' : 'terminal-only'
}
