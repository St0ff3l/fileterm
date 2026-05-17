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

export interface BaseProfile extends BaseEntity {
  type: SessionType
  host: string
  port: number
  group: string
}

export interface SshProfile extends BaseProfile {
  type: 'ssh'
  username: string
  authType: 'password' | 'privateKey'
  note?: string
  password?: string
  privateKeyPath?: string
  passphrase?: string
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
  status: 'queued' | 'running' | 'done' | 'failed'
  message?: string
}

export interface FileContentSnapshot {
  path: string
  name: string
  content: string
  source: 'local' | 'remote'
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

export interface SystemMetrics {
  ip: string
  uptime: string
  load: string
  cpuPercent: number
  memoryPercent: number
  memoryUsage: string
  memoryAppUsage?: string
  memoryCacheUsage?: string
  memoryKernelUsage?: string
  swapPercent: number
  swapUsage: string
  diskRows: Array<{ path: string; usage: string }>
  networkInterfaces: string[]
  activeNetworkInterface: string
  networkRates: NetworkRates
  networkSamples: NetworkSamplePoint[]
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
  connected?: boolean
  systemMetrics?: SystemMetrics
}

export interface WorkspaceSnapshot {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
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
  authType?: 'password' | 'privateKey'
  secure?: boolean
  encoding?: string
  backspaceKey?: string
  deleteKey?: string
  enableExecChannel?: boolean
}

export type ConnectionFormMode = 'create' | 'edit'

export type AppWindowMode = 'main' | 'connection-manager' | 'connection-form'

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

export interface TermdockDesktopApi {
  platform: string
  appName: string
  isDesktop: boolean
  openConnectionManagerWindow(): Promise<void>
  openConnectionFormWindow(mode: ConnectionFormMode, profileId?: string): Promise<void>
  closeCurrentWindow(): Promise<void>
  getSnapshot(): Promise<WorkspaceSnapshot>
  createFolder(name: string, parentId?: string): Promise<WorkspaceSnapshot>
  updateFolder(folderId: string, updates: Partial<ConnectionFolder>): Promise<WorkspaceSnapshot>
  deleteFolder(folderId: string): Promise<WorkspaceSnapshot>
  updateEntityOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot>
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
  readLocalFile(filePath: string): Promise<string>
  writeLocalFile(filePath: string, content: string): Promise<void>
  selectLocalFiles(defaultPath?: string): Promise<string[]>
  selectLocalDirectory(defaultPath?: string): Promise<string | null>
  queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot>
  uploadFile(tabId: string, localPath: string, remoteDirectory: string): Promise<WorkspaceSnapshot>
  downloadFile(tabId: string, remotePath: string, localDirectory: string): Promise<WorkspaceSnapshot>
  writeTerminal(tabId: string, data: string): Promise<void>
  resizeTerminal(tabId: string, cols: number, rows: number): Promise<void>
  openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot>
  readRemoteFile(tabId: string, targetPath: string): Promise<string>
  writeRemoteFile(tabId: string, targetPath: string, content: string): Promise<WorkspaceSnapshot>
  onTerminalData(listener: (payload: TerminalDataPayload) => void): () => void
  onTerminalState(listener: (payload: TerminalStatePayload) => void): () => void
  onWorkspaceSnapshot(listener: (snapshot: WorkspaceSnapshot) => void): () => void
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
  resize(cols: number, rows: number): Promise<void>
}

export interface FileSessionController extends SessionController {
  getRemotePath(): string
  listRemoteFiles(): Promise<RemoteFileItem[]>
  openRemotePath(path: string): Promise<RemoteFileItem[]>
  readRemoteFile(path: string): Promise<string>
  writeRemoteFile(path: string, content: string): Promise<void>
  uploadFile(localPath: string, remotePath: string, onProgress: (progress: number) => void): Promise<void>
  downloadFile(remotePath: string, localPath: string, onProgress: (progress: number) => void): Promise<void>
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
