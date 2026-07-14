import type { ConnectionProfile, CreateProfileInput, LocalFileItem, WorkspaceSnapshot } from '@fileterm/core'

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
  group: '默认',
  remotePath: '/',
  note: '',
  password: '',
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
  reconnectMode: 'none',
  secure: false,
  securityMode: 'none',
  proxy: { type: 'none', host: '', port: 1080, username: '' },
  proxyPassword: '',
  forwards: [],
  devicePath: '',
  baudRate: 115200,
  dataBits: 8,
  stopBits: 1,
  parity: 'none',
  flowControl: 'none'
}

export function profileToForm(profile: ConnectionProfile): CreateProfileInput {
  return {
    type: profile.type,
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    group: profile.group,
    remotePath: profile.remotePath,
    note: profile.note ?? '',
    password: profile.type === 'ssh' || profile.type === 'ftp' ? (profile.password ?? '') : '',
    trustedHostFingerprint: profile.type === 'ssh' ? (profile.trustedHostFingerprint ?? '') : '',
    authType: profile.type === 'ssh' ? (profile.authType === 'system' ? 'password' : profile.authType) : 'password',
    privateKeyId: profile.type === 'ssh' ? (profile.privateKeyId ?? '') : '',
    privateKeyPath: profile.type === 'ssh' ? (profile.privateKeyPath ?? '') : '',
    passphrase: profile.type === 'ssh' ? (profile.passphrase ?? '') : '',
    encoding: profile.type === 'ssh' ? (profile.encoding ?? 'UTF-8') : 'UTF-8',
    backspaceKey: profile.type === 'ssh' ? (profile.backspaceKey ?? 'ASCII') : 'ASCII',
    deleteKey: profile.type === 'ssh' ? (profile.deleteKey ?? 'VT220') : 'VT220',
    enableExecChannel: profile.type === 'ssh' ? (profile.enableExecChannel ?? true) : true,
    enableResourceMonitoring: profile.type === 'ssh' ? (profile.enableResourceMonitoring ?? true) : true,
    reconnectMode: profile.type === 'ssh' ? (profile.reconnectMode ?? 'none') : 'none',
    secure: profile.type === 'ftp' ? profile.secure : false,
    securityMode: profile.type === 'ftp' ? (profile.securityMode ?? (profile.secure ? 'explicit' : 'none')) : 'none',
    proxy:
      profile.type === 'ssh' || profile.type === 'telnet'
        ? (profile.proxy ?? { type: 'none', host: '', port: 1080 })
        : { type: 'none', host: '', port: 1080 },
    proxyPassword: profile.type === 'ssh' || profile.type === 'telnet' ? (profile.proxy?.password ?? '') : '',
    jumpProfileId: profile.type === 'ssh' ? profile.jumpProfileId : undefined,
    forwards: profile.type === 'ssh' ? (profile.forwards ?? []) : [],
    disableShellIntegration: profile.type === 'ssh' ? profile.disableShellIntegration : false,
    devicePath: profile.type === 'serial' ? profile.devicePath : '',
    baudRate: profile.type === 'serial' ? profile.baudRate : 115200,
    dataBits: profile.type === 'serial' ? profile.dataBits : 8,
    stopBits: profile.type === 'serial' ? profile.stopBits : 1,
    parity: profile.type === 'serial' ? profile.parity : 'none',
    flowControl: profile.type === 'serial' ? profile.flowControl : 'none'
  }
}
