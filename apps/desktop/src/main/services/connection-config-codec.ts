import path from 'node:path'
import type { ConnectionImportPreviewItem, ConnectionProfile, CreateProfileInput } from '@fileterm/core'
import { normalizeConnectionHost, validateConnectionHost } from '@fileterm/shared'

const SSH_SUPPORTED = new Set(['host', 'hostname', 'user', 'port', 'identityfile', 'proxyjump'])

export function previewSshConfig(text: string, group = '默认'): ConnectionImportPreviewItem[] {
  const blocks: Array<Record<string, string>> = []
  let current: Record<string, string> | undefined
  for (const raw of text.replace(/\r\n?/g, '\n').split('\n')) {
    const line = raw.replace(/\s+#.*$/, '').trim()
    if (!line) continue
    const separator = line.search(/\s/)
    if (separator < 1) continue
    const key = line.slice(0, separator).toLowerCase()
    const value = line.slice(separator).trim()
    if (key === 'host') {
      if (/[*?!]/.test(value) || value.split(/\s+/).length !== 1) {
        current = undefined
        continue
      }
      current = { host: value }
      blocks.push(current)
    } else if (current) current[key] = value
  }
  return blocks.map((block) => {
    const host = normalizeConnectionHost(block.hostname ?? '')
    const port = Number(block.port ?? 22)
    const unsupportedFields = Object.keys(block).filter((key) => !SSH_SUPPORTED.has(key))
    if (!host || !validateConnectionHost(host).valid || !Number.isInteger(port) || port < 1 || port > 65535) {
      return {
        name: block.host,
        type: 'ssh',
        status: 'invalid',
        reason: '缺少有效 HostName 或 Port',
        unsupportedFields
      }
    }
    return {
      name: block.host,
      type: 'ssh',
      host,
      port,
      username: block.user ?? '',
      status: 'ready',
      unsupportedFields,
      input: {
        type: 'ssh',
        name: block.host,
        host,
        port,
        username: block.user ?? '',
        group,
        remotePath: '/',
        authType: block.identityfile ? 'privateKey' : 'system',
        privateKeyPath: expandHome(block.identityfile),
        enableExecChannel: true,
        enableResourceMonitoring: true
      }
    }
  })
}

export function previewExternalConnectionJson(
  text: string,
  fallbackName: string,
  group = '默认'
): ConnectionImportPreviewItem[] {
  const raw = JSON.parse(text) as Record<string, unknown>
  if (Array.isArray(raw.profiles)) {
    return raw.profiles.flatMap((profile, index) =>
      previewExternalConnectionJson(JSON.stringify(profile), `${fallbackName}-${index + 1}`, group)
    )
  }
  const typeValue = String(raw.conection_type ?? raw.connection_type ?? raw.type ?? 'ssh').toLowerCase()
  const type = typeValue.includes('ftp')
    ? 'ftp'
    : typeValue.includes('telnet')
      ? 'telnet'
      : typeValue.includes('serial')
        ? 'serial'
        : 'ssh'
  const host = normalizeConnectionHost(String(raw.host ?? ''))
  const port = Number(raw.port ?? (type === 'ftp' ? 21 : type === 'telnet' ? 23 : 22))
  const name = String(raw.name ?? fallbackName)
  if (
    type !== 'serial' &&
    (!validateConnectionHost(host).valid || !Number.isInteger(port) || port < 1 || port > 65535)
  ) {
    return [{ name, type, status: 'invalid', reason: '主机或端口无效' }]
  }
  const unsupportedFields = Object.keys(raw).filter(
    (key) =>
      ![
        'id',
        'name',
        'description',
        'host',
        'port',
        'user_name',
        'password',
        'authentication_type',
        'terminal_encoding',
        'conection_type',
        'connection_type',
        'exec_channel_enable',
        'port_forwarding_list'
      ].includes(key)
  )
  const input: CreateProfileInput = {
    type,
    name,
    host,
    port,
    username: String(raw.user_name ?? raw.username ?? ''),
    group,
    remotePath: String(raw.remote_path ?? raw.remotePath ?? '/'),
    note: typeof raw.description === 'string' ? raw.description : typeof raw.note === 'string' ? raw.note : undefined,
    password: typeof raw.password === 'string' ? raw.password : undefined,
    privateKeyPath: stringValue(raw.private_key_path ?? raw.privateKeyPath),
    passphrase: stringValue(raw.passphrase),
    authType: mapAuth(raw.authentication_type ?? raw.authType),
    encoding: typeof raw.terminal_encoding === 'string' ? raw.terminal_encoding : 'UTF-8',
    enableExecChannel: raw.exec_channel_enable !== false,
    enableResourceMonitoring: true
  }
  return [{ name, type, host, port, username: input.username, status: 'ready', unsupportedFields, input }]
}

export function exportProfiles(
  profiles: ConnectionProfile[],
  format: 'fileterm' | 'compatible',
  includeCredentials = false
) {
  if (format === 'fileterm')
    return {
      schemaVersion: 1,
      generatedAt: new Date().toISOString(),
      profiles: profiles.map((profile) => serializeProfile(profile, includeCredentials))
    }
  return profiles.map((profile) => ({
    id: profile.id,
    name: profile.name,
    description: 'note' in profile ? profile.note : undefined,
    conection_type: profile.type,
    host: profile.host,
    port: profile.port,
    user_name: profile.username,
    terminal_encoding: 'encoding' in profile ? profile.encoding : undefined,
    authentication_type: profile.type === 'ssh' ? profile.authType : undefined,
    password: includeCredentials && (profile.type === 'ssh' || profile.type === 'ftp') ? profile.password : undefined,
    private_key_path: includeCredentials && profile.type === 'ssh' ? profile.privateKeyPath : undefined,
    passphrase: includeCredentials && profile.type === 'ssh' ? profile.passphrase : undefined,
    exec_channel_enable: profile.type === 'ssh' ? profile.enableExecChannel : undefined,
    port_forwarding_list: profile.type === 'ssh' ? profile.forwards : undefined,
    unsupported_fields: profile.type === 'ssh' && profile.jumpProfileId ? ['jumpProfileId'] : []
  }))
}

function mapAuth(value: unknown): CreateProfileInput['authType'] {
  const text = String(value ?? '').toLowerCase()
  return text.includes('interactive')
    ? 'keyboard-interactive'
    : text.includes('key')
      ? 'privateKey'
      : text.includes('system') || text.includes('agent')
        ? 'system'
        : 'password'
}

function stringValue(value: unknown) {
  return typeof value === 'string' ? value : undefined
}

function expandHome(value: string | undefined) {
  return value?.startsWith('~/') ? path.join(process.env.HOME ?? '', value.slice(2)) : value
}
function serializeProfile(profile: ConnectionProfile, includeCredentials: boolean) {
  if (includeCredentials) return profile
  const {
    password: _password,
    privateKeyPath: _key,
    passphrase: _passphrase,
    ...safe
  } = profile as ConnectionProfile & { password?: string; privateKeyPath?: string; passphrase?: string }
  if ((safe.type === 'ssh' || safe.type === 'telnet') && safe.proxy?.password) {
    const { password: _proxyPassword, ...proxy } = safe.proxy
    return { ...safe, proxy }
  }
  return safe
}
