export type SessionType = 'ssh' | 'ftp'

export type TabLayout = 'terminal-file' | 'file-only'

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
  host: string
  port: number
  group: string
}

export interface SshProfile extends BaseProfile {
  type: 'ssh'
  username: string
  authType: 'password' | 'privateKey' | 'system'
  note?: string
  password?: string
  privateKeyPath?: string
  passphrase?: string
  trustedHostFingerprint?: string
  sftpEnabled: boolean
  remotePath: string
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
}

export interface FtpProfile extends BaseProfile {
  type: 'ftp'
  username: string
  note?: string
  password?: string
  secure: boolean
  remotePath: string
}

export type ConnectionProfile = SshProfile | FtpProfile

export interface WorkspaceTab {
  id: string
  sessionType: SessionType
  profileId: string
  title: string
  layout: TabLayout
  status: TabStatus
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

export interface TransferTask {
  id: string
  direction: 'upload' | 'download'
  name: string
  progress: number
  status: 'queued' | 'running' | 'done' | 'failed' | 'canceled'
  message?: string
  speed?: string
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
  ip: string
  uptime: string
  uptimeSeconds?: number
  load: string
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
  swapPercent: number
  swapUsage: string
  swapBreakdown: ResourceUsageBreakdown
  diskRows: Array<{ path: string; usage: string }>
  fileSystemRows: FileSystemRow[]
  networkInterfaces: string[]
  activeNetworkInterface: string
  networkRates: NetworkRates
  networkSamples: NetworkSamplePoint[]
  networkInterfaceRows: NetworkInterfaceRow[]
  networkRatesByInterface?: Record<string, NetworkRates>
  networkSamplesByInterface?: Record<string, NetworkSamplePoint[]>
  topProcesses: SidebarProcessItem[]
}

export interface SessionSnapshot {
  profileId: string
  accessHost?: string
  summary: string
  terminalTranscript?: string
  remotePath: string
  remoteFiles: RemoteFileItem[]
  fileAccessMode?: 'user' | 'root'
  sudoUser?: string
  hasReusableSudoAuth?: boolean
  connected?: boolean
  systemMetrics?: SystemMetrics
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
  privateKeyPath?: string
  passphrase?: string
  authType?: 'password' | 'privateKey' | 'system'
  trustedHostFingerprint?: string
  secure?: boolean
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
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

export type SshInteractionRequest = SshHostVerificationRequest | SshCredentialsPromptRequest
export type SshInteractionDraft =
  | Omit<SshHostVerificationRequest, 'requestId' | 'tabId' | 'profileId'>
  | Omit<SshCredentialsPromptRequest, 'requestId' | 'tabId' | 'profileId'>

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

export type SshInteractionResponse = SshHostVerificationResponse | SshCredentialsPromptResponse

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

export interface TermdockDesktopApi {
  platform: string
  appName: string
  isDesktop: boolean
  readClipboardText(): Promise<string>
  writeClipboardText(text: string): Promise<void>
  getUiPreferences(): Promise<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>
  setUiPreferences(input: { theme?: 'default-dark' | 'default-light'; locale?: 'zhCN' | 'enUS' }): Promise<{ theme: 'default-dark' | 'default-light'; locale: 'zhCN' | 'enUS' }>
  openConnectionManagerWindow(): Promise<void>
  openCommandManagerWindow(): Promise<void>
  openConnectionFormWindow(mode: ConnectionFormMode, profileId?: string): Promise<void>
  openCommandFormWindow(mode: ConnectionFormMode, commandId?: string, folderId?: string): Promise<void>
  openFileEditorWindow(input: FileEditorWindowInput): Promise<void>
  openLogsDirectory(): Promise<void>
  minimizeCurrentWindow(): Promise<void>
  toggleMaximizeCurrentWindow(): Promise<void>
  closeCurrentWindow(): Promise<void>
  getSnapshot(): Promise<WorkspaceSnapshot>
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
  executeCommandTemplate(tabId: string, commandId: string, args?: string[], options?: CommandExecutionOptions): Promise<CommandExecutionResult>
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
  clearTransfers(transferIds: string[]): Promise<WorkspaceSnapshot>
  uploadFile(tabId: string, localPath: string, remoteDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot>
  downloadFile(tabId: string, remotePath: string, localDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot>
  downloadRemotePath(tabId: string, remotePath: string, targetType: RemoteFileItem['type'], localDirectory: string, options?: TransferTargetOptions): Promise<WorkspaceSnapshot>
  setRemoteFileAccessMode(tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions): Promise<WorkspaceSnapshot>
  writeTerminal(tabId: string, data: string): Promise<void>
  resizeTerminal(tabId: string, cols: number, rows: number, width: number, height: number): Promise<void>
  openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot>
  readRemoteFile(tabId: string, targetPath: string, encoding?: string): Promise<string>
  writeRemoteFile(tabId: string, targetPath: string, content: string, encoding?: string): Promise<WorkspaceSnapshot>
  createRemoteDirectory(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot>
  createRemoteFile(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot>
  copyRemotePath(tabId: string, targetPath: string, destinationPath: string, targetType: RemoteFileItem['type']): Promise<WorkspaceSnapshot>
  moveRemotePath(tabId: string, targetPath: string, destinationPath: string): Promise<WorkspaceSnapshot>
  renameRemotePath(tabId: string, targetPath: string, newName: string): Promise<WorkspaceSnapshot>
  deleteRemotePath(tabId: string, targetPath: string, targetType: RemoteFileItem['type']): Promise<WorkspaceSnapshot>
  resolveSshInteraction(requestId: string, response: SshInteractionResponse): Promise<void>
  changeRemotePermissions(tabId: string, targetPath: string, options: PermissionChangeOptions): Promise<WorkspaceSnapshot>
  onTerminalData(listener: (payload: TerminalDataPayload) => void): () => void
  onTerminalState(listener: (payload: TerminalStatePayload) => void): () => void
  onWorkspaceSnapshot(listener: (snapshot: WorkspaceSnapshot) => void): () => void
  onSshInteraction(listener: (request: SshInteractionRequest) => void): () => void
  onWindowCloseRequest(listener: (event: { isQuit: boolean }) => void): () => void
  confirmCloseWindow(action: 'quit' | 'hide' | 'cancel'): Promise<void>
}

export interface SessionController {
  readonly id: string
  readonly type: SessionType
  connect(): Promise<void>
  disconnect(): Promise<void>
  getSummary(): string
}

export interface ShellSessionController extends SessionController {
  readonly type: 'ssh'
  getTerminalTranscript(): string
  write(data: string): Promise<void>
  resize(cols: number, rows: number, width: number, height: number): Promise<void>
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
  uploadFile(localPath: string, remotePath: string, onProgress: (progress: TransferProgress) => void): Promise<void>
  downloadFile(remotePath: string, localPath: string, onProgress: (progress: TransferProgress) => void): Promise<void>
}

export interface SshSessionController extends ShellSessionController, FileSessionController {
  readonly type: 'ssh'
}

export interface FtpSessionController extends FileSessionController {
  readonly type: 'ftp'
}

export const createTabLayout = (profile: ConnectionProfile): TabLayout => {
  return profile.type === 'ssh' ? 'terminal-file' : 'file-only'
}
