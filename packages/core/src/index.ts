export type SessionType = 'ssh' | 'ftp' | 'telnet' | 'serial'

export type FtpSecurityMode = 'none' | 'explicit' | 'implicit'

export type TabLayout = 'terminal-file' | 'file-only' | 'terminal-only'

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

export interface SshProfile extends NetworkProfile {
  type: 'ssh'
  username: string
  authType: SshAuthType
  note?: string
  password?: string
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
  reconnectMode?: 'none' | 'enter' | 'auto'
  proxy?: ProxyConfig
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
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
  sessionType: SessionType
  profileId: string
  title: string
  layout: TabLayout
  status: TabStatus
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
  action: 'upload' | 'download'
  message: string
  imported?: number
  skipped?: number
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
  reconnectMode?: 'none' | 'enter' | 'auto'
  proxy?: ProxyConfig
  proxyPassword?: string
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
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
}

export interface RemoteFileAccessOptions {
  sudoUser?: string
  sudoPassword?: string
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
  getUiPreferences(): Promise<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>
  setUiPreferences(input: {
    theme?: 'default-dark' | 'default-light'
    locale?: 'zhCN' | 'enUS'
  }): Promise<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>
  getUiStateItem(key: string): Promise<string | null>
  setUiStateItem(key: string, value: string): Promise<void>
  removeUiStateItem(key: string): Promise<void>
  openConnectionManagerWindow(): Promise<void>
  openCommandManagerWindow(): Promise<void>
  openConnectionFormWindow(mode: ConnectionFormMode, profileId?: string): Promise<void>
  openCommandFormWindow(mode: ConnectionFormMode, commandId?: string, folderId?: string): Promise<void>
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
  onWindowMaximizedChange(listener: (isMaximized: boolean) => void): () => void
  onUiPreferencesChanged(
    listener: (preferences: { theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }) => void
  ): () => void
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
  uploadWebDavSync(): Promise<WebDavSyncResult>
  downloadWebDavSync(): Promise<WebDavSyncResult>
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
  listLocalDirectory(dirPath?: string): Promise<DirectorySnapshot<LocalFileItem>>
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
  onWindowCloseRequest(listener: (event: { isQuit: boolean }) => void): () => void
  onRequestCloseActiveWorkspaceItem(listener: () => void): () => void
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
