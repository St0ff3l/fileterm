import type {
  ConnectionProfile,
  CreateProfileInput,
  LocalFileItem,
  WorkspaceSnapshot
} from '@fileterm/core'

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
  privateKeyPath: '',
  passphrase: '',
  trustedHostFingerprint: '',
  authType: 'password',
  encoding: 'UTF-8',
  backspaceKey: 'ASCII',
  deleteKey: 'VT220',
  enableExecChannel: true,
  secure: false
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
    password: profile.password ?? '',
    trustedHostFingerprint: profile.type === 'ssh' ? profile.trustedHostFingerprint ?? '' : '',
    authType: profile.type === 'ssh'
      ? (profile.authType === 'system' ? 'password' : profile.authType)
      : 'password',
    privateKeyPath: profile.type === 'ssh' ? profile.privateKeyPath ?? '' : '',
    passphrase: profile.type === 'ssh' ? profile.passphrase ?? '' : '',
    encoding: profile.type === 'ssh' ? profile.encoding ?? 'UTF-8' : 'UTF-8',
    backspaceKey: profile.type === 'ssh' ? profile.backspaceKey ?? 'ASCII' : 'ASCII',
    deleteKey: profile.type === 'ssh' ? profile.deleteKey ?? 'VT220' : 'VT220',
    enableExecChannel: profile.type === 'ssh' ? profile.enableExecChannel ?? true : true,
    secure: profile.type === 'ftp' ? profile.secure : false
  }
}
