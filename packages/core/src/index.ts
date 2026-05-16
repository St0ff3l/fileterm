export type SessionType = 'ssh' | 'ftp'

export type TabLayout = 'terminal-file' | 'file-only'

export type TabStatus = 'idle' | 'connecting' | 'connected' | 'error' | 'closed'

export interface BaseProfile {
  id: string
  type: SessionType
  name: string
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
}

export interface SidebarProcessItem {
  memory: string
  cpu: string
  command: string
}

export interface NetworkSamplePoint {
  rx: number
  tx: number
}

export interface SystemMetrics {
  ip: string
  uptime: string
  load: string
  cpuPercent: number
  memoryPercent: number
  memoryUsage: string
  swapPercent: number
  swapUsage: string
  diskRows: Array<{ path: string; usage: string }>
  networkInterfaces: string[]
  activeNetworkInterface: string
  networkRates: {
    rx: string
    tx: string
  }
  networkSamples: NetworkSamplePoint[]
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
