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
  /** Non-secret indicator for redacted desktop snapshots. */
  hasSavedPassword?: boolean
  /** Non-secret indicator for a saved privileged-command password. */
  hasSavedSudoPassword?: boolean
  /** Non-secret indicator for a saved `su` password. */
  hasSavedSuPassword?: boolean
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

export type SshAuthType = 'password' | 'privateKey' | 'system' | 'keyboard-interactive'

/** 资源监控采集间隔，单位为秒。 */
export type ResourceMonitoringIntervalSeconds = 1 | 5 | 15 | 30 | 60

/** SSH connection behavior shared by newly created connections. */
export interface SshConnectionDefaults {
  useEmptyPassword: boolean
  enableExecChannel: boolean
  enableResourceMonitoring: boolean
  resourceMonitoringIntervalSeconds: ResourceMonitoringIntervalSeconds
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
  reconnectMode: 'none',
  legacyAlgorithms: false
}

/** Scope exposed to externally launched MCP Agents such as Codex and Claude. */
export type McpConnectionScope = 'all-saved-connections' | 'active-session' | 'default-connection'

/** Whether an external MCP Agent may request state-changing operations. */
export type McpOperationPolicy = 'read-only' | 'approved-operations'

/** Non-secret local policy for an external MCP Agent connection. */
export interface McpAgentPreferences {
  connectionScope: McpConnectionScope
  operationPolicy: McpOperationPolicy
  /** Saved connection profile used when `connectionScope` is `default-connection`. */
  defaultProfileId?: string
}

export const DEFAULT_MCP_AGENT_PREFERENCES: McpAgentPreferences = {
  connectionScope: 'all-saved-connections',
  operationPolicy: 'approved-operations'
}

export type McpAgentClientId = 'claude-code' | 'codex-cli'

/** Local client discovery result. Detection only reads PATH and never runs the client. */
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
  sftpEnabled: boolean
  remotePath: string
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
  enableResourceMonitoring?: boolean
  resourceMonitoringIntervalSeconds?: ResourceMonitoringIntervalSeconds
  reconnectMode?: 'none' | 'enter' | 'auto'
  connectionOverrides?: SshConnectionOverrides
  proxy?: ProxyConfig
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
  /** Reuse the saved SSH login password for sudo when explicitly enabled. */
  sudoSameAsLogin?: boolean
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
  remotePath: string
}

export interface TelnetProfile extends NetworkProfile {
  type: 'telnet'
  note?: string
  encoding?: string
  proxy?: ProxyConfig
}

export interface SerialProfile extends BaseProfile {
  type: 'serial'
  devicePath: string
  baudRate: number
  dataBits: 5 | 6 | 7 | 8
  stopBits: 1 | 2
  parity: 'none' | 'odd' | 'even' | 'mark' | 'space'
  flowControl: 'none' | 'hardware' | 'software'
  encoding?: string
  note?: string
}

export type ConnectionProfile = SshProfile | FtpProfile | TelnetProfile | SerialProfile

export const getConnectionCapabilities = (profile: Pick<ConnectionProfile, 'type'>): ConnectionCapabilities => {
  if (profile.type === 'ssh') {
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

export type RemoteSystemPlatform = 'linux' | 'busybox' | 'windows' | 'unknown'

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

  return {
    ...nextMetrics,
    networkSamples: [...previousSamples, nextPoint].slice(-historyLimit),
    networkSamplesByInterface: mergedByInterface
  }
}

export interface SessionSnapshot {
  profileId: string
  /** Monotonic terminal-target identity; unchanged by ordinary output chunks. */
  aiSessionRevision?: string
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
  reconnectMode?: 'none' | 'enter' | 'auto'
}

export interface WorkspaceSnapshot {
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

export interface WebDavSyncResult {
  action: 'test' | 'upload' | 'download'
  message: string
  imported?: number
  updated?: number
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
  password?: string
  /** Explicitly authenticate with an empty SSH password instead of using a saved password. */
  useEmptyPassword?: boolean
  privateKeyId?: string
  privateKeyPath?: string
  passphrase?: string
  authType?: SshAuthType
  trustedHostFingerprint?: string
  secure?: boolean
  securityMode?: FtpSecurityMode
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
  enableResourceMonitoring?: boolean
  resourceMonitoringIntervalSeconds?: ResourceMonitoringIntervalSeconds
  reconnectMode?: 'none' | 'enter' | 'auto'
  connectionOverrides?: SshConnectionOverrides
  proxy?: ProxyConfig
  proxyPassword?: string
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
  /** Transient only; persisted by Rust in profile-secrets.json. */
  sudoPassword?: string
  /** Transient only; persisted by Rust in profile-secrets.json. */
  suPassword?: string
  /** Reuse the saved SSH login password for sudo when explicitly enabled. */
  sudoSameAsLogin?: boolean
  /** 兼容老服务器：追加 SHA-1 类 MAC/KEX 算法到偏好列表末尾（SHA-2 仍优先） */
  legacyAlgorithms?: boolean
  devicePath?: string
  baudRate?: number
  dataBits?: 5 | 6 | 7 | 8
  stopBits?: 1 | 2
  parity?: SerialProfile['parity']
  flowControl?: SerialProfile['flowControl']
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

/**
 * A task-local prompt from an isolated SSH exec PTY. This is not a terminal
 * input request: the response goes only to the one MCP/CLI task that raised
 * it and is never added to the terminal transcript or returned to the agent.
 */
export interface RemoteExecInteractionRequest {
  requestId: string
  tabId: string
  command: string
  host: string
  shellUser?: string
  cwd?: string
  prompt: string
  attempt: number
  maxAttempts: number
  inputKind: 'secret' | 'text'
}

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

export type ActionApprovalSource = 'mcp' | 'ai-review' | 'ai-copilot'

/** One-time in-app approval shared by MCP and AI Review Mode. */
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

export interface UiPreferences {
  theme: 'default-dark' | 'default-light'
  locale: 'zhCN' | 'enUS'
  autoCheckUpdates: boolean
  terminalZoomLocked: boolean
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
  autoCheckUpdates?: boolean
  terminalZoomLocked?: boolean
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

export interface AiAutoModeThresholds {
  maxToolCallsPerSession: number
  maxDestructiveCallsPerSession: number
  maxPrivilegedCallsPerSession: number
  maxTotalExecDurationSecs: number
}

/**
 * Renderer-facing safety floor for full-auto guardrails. Rust remains the
 * authority and rejects values below this floor; the shared constant keeps
 * advanced settings aligned with the documented defaults.
 */
export const DEFAULT_AI_AUTO_MODE_THRESHOLDS: Readonly<AiAutoModeThresholds> = {
  maxToolCallsPerSession: 20,
  maxDestructiveCallsPerSession: 5,
  maxPrivilegedCallsPerSession: 3,
  maxTotalExecDurationSecs: 600
}

export interface AiAutoModeGuardrailState {
  sessionToolCallCount: number
  sessionDestructiveCount: number
  sessionPrivilegedCount: number
  sessionTotalExecDurationSecs: number
  thresholds: AiAutoModeThresholds
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

export interface SetAiAutoModeThresholdsInput {
  thresholds: AiAutoModeThresholds
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

export type AiReviewOutcome =
  | 'completed'
  | 'rejected'
  | 'approval-dismissed'
  | 'approval-timed-out'
  | 'target-changed'
  | 'command-timed-out'
  | 'failed'

/** Local audit metadata for a single user-approved SSH exec invocation. */
export interface AiReviewRecord {
  id: string
  commandId: string
  command: string
  risk: AiCommandRisk
  target: AiContextTarget
  timeoutMs: number
  requestedAt: string
  approvedAt?: string
  completedAt: string
  outcome: AiReviewOutcome
  exitCode?: number
  timedOut: boolean
  outputTruncated: boolean
  output?: string
  error?: string
}

/** A structured, locally validated command proposal; it is never an execution request. */
export interface AiCommandSuggestion {
  id: string
  command: string
  explanation?: string
  risk: AiCommandRisk
  multiline: boolean
  target: AiContextTarget
}

/** A message stored locally for an AI Copilot conversation. */
export interface AiMessage {
  id: string
  role: 'user' | 'assistant' | 'review'
  content: string
  createdAt: string
  /** Present only for an explicitly approved L2 user turn; never contains raw transcript text. */
  context?: AiContextAttachment
  /**
   * Legacy command-card data retained for existing local conversations.
   * New Copilot turns use the three permission modes instead of selecting a
   * separate command-card response mode.
   */
  commands?: AiCommandSuggestion[]
  /** Present only for one-time AI Review Mode audit messages. */
  review?: AiReviewRecord
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
 * @deprecated Kept as a wire-compatibility field while older local callers
 * and fixtures migrate. The renderer no longer exposes a command-card mode;
 * new turns are selected through AiCopilotMode.
 */
export type AiChatResponseMode = 'chat' | 'command-proposal'

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
  /** @deprecated Compatibility only; the renderer does not send this field. */
  responseMode?: AiChatResponseMode
  /** Optional for old callers; Rust defaults and validates the active mode. */
  mode?: AiCopilotMode
}

/** Retries the latest user turn without duplicating it in local history. */
export interface RetryAiChatInput {
  conversationId: string
  providerId: string
  modelOverride?: string
  contextSnapshotId?: string
  /** @deprecated Compatibility only; the renderer does not send this field. */
  responseMode?: AiChatResponseMode
  mode?: AiCopilotMode
}

export interface AiChatRequest {
  requestId: string
  conversationId: string
  userMessageId: string
  assistantMessageId: string
}

export interface AiCommandInsertInput {
  commandId: string
}

/** Rust approved the target-bound command for a UI-only terminal input handoff. */
export interface AiCommandInsertResult {
  tabId: string
  command: string
}

export interface RunAiReviewInput {
  commandId: string
}

export interface AiReviewExecution {
  conversation: AiConversation
  review: AiReviewRecord
}

export interface AiToolCallProposal {
  id: string
  toolName: 'fileterm_execute_remote_command'
  command: string
  risk: AiCommandRisk
  target: AiContextTarget
  explanation?: string
}

export interface AiToolCallResult {
  proposalId: string
  status: 'approved' | 'rejected' | 'auto-blocked' | 'executed' | 'failed' | 'timeout' | 'target-changed' | 'invalid'
  exitCode?: number
  stdout?: string
  stderr?: string
  durationMs?: number
  reason?: string
}

/** Per-request stream events; never emitted through a global application event. */
export type AiStreamEvent =
  | { type: 'started'; requestId: string; messageId: string }
  | { type: 'text-delta'; text: string }
  | { type: 'command'; command: AiCommandSuggestion }
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
  | 'AI_AUTO_MODE_UNAVAILABLE'
  | 'AI_AUTO_MODE_INVALID_THRESHOLDS'
  | 'AI_AUTO_MODE_BLOCKED_COMMAND'
  | 'AI_AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED'
  | 'AI_AUTO_MODE_SESSION_LIMIT_REACHED'
  | 'AI_AUTO_MODE_RISK_LIMIT_REACHED'
  | 'AI_AUTO_MODE_DURATION_LIMIT_REACHED'
  | 'AI_AUTO_MODE_TARGET_CHANGED'
  | 'AI_TOOL_CALL_REJECTED'
  | 'AI_TOOL_CALL_INVALID'
  | 'AI_TOOL_LOOP_LIMIT'
  | 'AI_CONVERSATION_LIMIT'
  | 'AI_CONVERSATION_NOT_FOUND'
  | 'AI_CONVERSATION_INVALID_INPUT'

export interface AiCommandError {
  code: AiErrorCode
  message: string
  retryable: boolean
  httpStatus?: number
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
  deleteAiConversation(conversationId: string): Promise<void>
  getAiCopilotModeState(): Promise<AiCopilotModeState>
  setAiCopilotMode(input: SetAiCopilotModeInput): Promise<AiCopilotModeState>
  setAiContextAttach(input: SetAiContextAttachInput): Promise<AiCopilotModeState>
  getAiAutoModeThresholds(): Promise<AiAutoModeThresholds>
  setAiAutoModeThresholds(input: SetAiAutoModeThresholdsInput): Promise<AiCopilotModeState>
  resetAiAutoModeSessionCounts(): Promise<AiCopilotModeState>
  createAiContextPreview(input: CreateAiContextPreviewInput): Promise<AiContextPreview>
  startAiChat(input: StartAiChatInput, onEvent: (event: AiStreamEvent) => void): Promise<AiChatRequest>
  retryAiChat(input: RetryAiChatInput, onEvent: (event: AiStreamEvent) => void): Promise<AiChatRequest>
  cancelAiChat(requestId: string): Promise<void>
  insertAiCommand(input: AiCommandInsertInput): Promise<AiCommandInsertResult>
  runAiReview(input: RunAiReviewInput): Promise<AiReviewExecution>
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
  onFileEditorCloseRequest(listener: () => void): () => void
  requestQuitApp(): Promise<void>
  getSnapshot(): Promise<WorkspaceSnapshot>
  getConnectionLibrary(): Promise<ConnectionLibrarySnapshot>
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
  uploadWebDavSync(): Promise<WebDavSyncResult>
  downloadWebDavSync(): Promise<WebDavSyncResult>
  getS3BackupConfig(): Promise<S3BackupConfig>
  saveS3BackupConfig(input: S3BackupConfigInput): Promise<S3BackupConfig>
  testS3Backup(): Promise<S3BackupResult>
  uploadS3Backup(): Promise<S3BackupResult>
  downloadS3Backup(): Promise<S3BackupResult>
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
  executeInteractiveRemoteCommand(
    tabId: string,
    expectedSessionRevision: string,
    command: string,
    cwd?: string,
    timeoutMs?: number
  ): Promise<{
    output: string
    exitCode: number | null
    timedOut: boolean
    outputTruncated: boolean
    inputRequired: boolean
    inputKind?: 'secret' | 'text'
    /** Number of local secure-input rounds; answers themselves never leave FileTerm. */
    interactionCount?: number
  }>
  getTerminalCommandHistory(profileId: string): Promise<TerminalCommandHistoryEntry[]>
  setTerminalCommandHistory(profileId: string, entries: TerminalCommandHistoryEntry[]): Promise<void>
  getCommandSendPreferences(): Promise<CommandSendPreferences>
  setCommandSendPreferences(preferences: CommandSendPreferences): Promise<void>
  createProfile(input: CreateProfileInput): Promise<WorkspaceSnapshot>
  updateProfile(profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot>
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
  deleteRemotePath(tabId: string, targetPath: string, targetType: RemoteFileItem['type']): Promise<WorkspaceSnapshot>
  resolveSshInteraction(requestId: string, response: SshInteractionResponse): Promise<void>
  resolveRemoteExecInteraction(requestId: string, cancelled: boolean, value?: string): Promise<void>
  setRemoteExecInteractionRendererReady(registrationId: string, ready: boolean): Promise<void>
  resolveSudoPasswordPrompt(requestId: string, cancelled: boolean, value?: string, save?: boolean): Promise<void>
  setSudoPasswordPromptRendererReady(registrationId: string, ready: boolean): Promise<void>
  resolveBackupPassword(requestId: string, cancelled: boolean, value?: string): Promise<void>
  setBackupPasswordRendererReady(registrationId: string, ready: boolean): Promise<void>
  resolveActionApproval(requestId: string, approved: boolean): Promise<void>
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
  onWorkspaceSnapshot(listener: (snapshot: WorkspaceSnapshot) => void): () => void
  onSessionMetrics(listener: (payload: SessionMetricsUpdate) => void): () => void
  onSshInteraction(listener: (request: SshInteractionRequest) => void): () => void
  /** Resolves only after the main renderer has registered its secure-input listener. */
  onRemoteExecInteraction(listener: (request: RemoteExecInteractionRequest) => void): Promise<() => void>
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

export interface AppUpdateStatus {
  state: AppUpdateState
  currentVersion: string
  updateMode?: AppUpdateMode
  availableVersion?: string
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
