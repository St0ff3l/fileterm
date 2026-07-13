import { createHash } from 'node:crypto'
import { chmod, mkdir, readFile, rename, writeFile } from 'node:fs/promises'
import path from 'node:path'
import type { ConnectionImportResult, ConnectionProfile, WebDavSyncConfig, WebDavSyncResult } from '@fileterm/core'
import { exportProfiles, previewExternalConnectionJson } from './connection-config-codec.js'

type StoredConfig = WebDavSyncConfig & { password?: string; contentHash?: string }

const DEFAULT_CONFIG: WebDavSyncConfig = {
  enabled: false,
  url: '',
  remotePath: 'fileterm-connections.json'
}

/** Manual, conflict-aware WebDAV profile backup. Credentials never leave main. */
export class WebDavSyncService {
  private readonly configPath: string
  private readonly profiles: () => Promise<ConnectionProfile[]>
  private readonly importProfiles: (
    items: ReturnType<typeof previewExternalConnectionJson>
  ) => Promise<ConnectionImportResult>

  constructor(
    baseDir: string,
    profiles: () => Promise<ConnectionProfile[]>,
    importProfiles: (items: ReturnType<typeof previewExternalConnectionJson>) => Promise<ConnectionImportResult>
  ) {
    this.configPath = path.join(baseDir, 'webdav-sync.json')
    this.profiles = profiles
    this.importProfiles = importProfiles
  }

  async getConfig(): Promise<WebDavSyncConfig> {
    const config = await this.readConfig()
    return publicConfig(config)
  }

  async saveConfig(input: WebDavSyncConfig & { password?: string }): Promise<WebDavSyncConfig> {
    const url = validateBaseUrl(input.url, input.allowInsecureTls)
    const remotePath = normalizeRemotePath(input.remotePath)
    const previous = await this.readConfig()
    const next: StoredConfig = {
      ...previous,
      enabled: input.enabled,
      url,
      username: input.username?.trim() || undefined,
      remotePath,
      allowInsecureTls: input.allowInsecureTls === true,
      ...(input.password === undefined ? {} : { password: input.password || undefined })
    }
    await this.writeConfig(next)
    return publicConfig(next)
  }

  async upload(): Promise<WebDavSyncResult> {
    const config = await this.readConfigured()
    const profiles = await this.profiles()
    const payload = JSON.stringify(exportProfiles(profiles, 'fileterm'), null, 2)
    const hash = sha256(payload)
    const remote = this.remoteUrl(config)
    const head = await this.request(remote, config, { method: 'HEAD', allow404: true })
    const remoteEtag = head.headers.get('etag') ?? undefined
    if (remoteEtag && !config.lastEtag) {
      throw new Error('远端已存在配置包。请先下载并确认内容，再上传以避免首次同步覆盖。')
    }
    if (config.lastEtag && remoteEtag && config.lastEtag !== remoteEtag && config.contentHash !== hash) {
      throw new Error('远端配置自上次同步后已变更。请先下载并确认冲突，再上传。')
    }
    const response = await this.request(remote, config, {
      method: 'PUT',
      body: payload,
      headers: {
        'content-type': 'application/json; charset=utf-8',
        ...(remoteEtag ? { 'if-match': remoteEtag } : { 'if-none-match': '*' })
      }
    })
    if (!response.ok) throw new Error(`WebDAV 上传失败 (${response.status})`)
    const next: StoredConfig = {
      ...config,
      lastEtag: response.headers.get('etag') ?? remoteEtag,
      lastSyncedAt: new Date().toISOString(),
      contentHash: hash
    }
    await this.writeConfig(next)
    return { action: 'upload', message: '连接配置已上传到 WebDAV。' }
  }

  async download(): Promise<WebDavSyncResult> {
    const config = await this.readConfigured()
    const response = await this.request(this.remoteUrl(config), config, { method: 'GET' })
    if (!response.ok) throw new Error(`WebDAV 下载失败 (${response.status})`)
    const text = await response.text()
    if (Buffer.byteLength(text, 'utf8') > 5 * 1024 * 1024) throw new Error('WebDAV 配置包超过 5 MB 限制')
    const items = previewExternalConnectionJson(text, 'webdav-sync')
    const result = await this.importProfiles(items)
    const next: StoredConfig = {
      ...config,
      lastEtag: response.headers.get('etag') ?? undefined,
      lastSyncedAt: new Date().toISOString(),
      contentHash: sha256(text)
    }
    await this.writeConfig(next)
    return {
      action: 'download',
      message: `已从 WebDAV 导入 ${result.imported} 个连接；跳过 ${result.skipped} 个重复项。`,
      imported: result.imported,
      skipped: result.skipped
    }
  }

  private async readConfigured() {
    const config = await this.readConfig()
    if (!config.enabled) throw new Error('请先启用 WebDAV 配置同步')
    validateBaseUrl(config.url, config.allowInsecureTls)
    return config
  }

  private remoteUrl(config: StoredConfig) {
    return new URL(normalizeRemotePath(config.remotePath), `${config.url.replace(/\/$/, '')}/`).toString()
  }

  private async request(url: string, config: StoredConfig, options: RequestInit & { allow404?: boolean }) {
    const headers = new Headers(options.headers)
    if (config.username)
      headers.set(
        'authorization',
        `Basic ${Buffer.from(`${config.username}:${config.password ?? ''}`).toString('base64')}`
      )
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), 20_000)
    try {
      const response = await fetch(url, { ...options, headers, signal: controller.signal })
      if (options.allow404 && response.status === 404) return response
      return response
    } finally {
      clearTimeout(timeout)
    }
  }

  private async readConfig(): Promise<StoredConfig> {
    try {
      return { ...DEFAULT_CONFIG, ...(JSON.parse(await readFile(this.configPath, 'utf8')) as Partial<StoredConfig>) }
    } catch {
      return { ...DEFAULT_CONFIG }
    }
  }

  private async writeConfig(config: StoredConfig) {
    await mkdir(path.dirname(this.configPath), { recursive: true })
    const temporary = `${this.configPath}.tmp`
    await writeFile(temporary, JSON.stringify(config, null, 2), { encoding: 'utf8', mode: 0o600 })
    await chmod(temporary, 0o600).catch(() => undefined)
    await rename(temporary, this.configPath)
  }
}

function publicConfig({ password: _password, contentHash: _contentHash, ...config }: StoredConfig): WebDavSyncConfig {
  return config
}

function validateBaseUrl(value: string, allowInsecureTls?: boolean) {
  const url = new URL(value.trim())
  if (url.protocol !== 'https:' && !(allowInsecureTls && url.protocol === 'http:')) {
    throw new Error('WebDAV 地址必须使用 HTTPS；HTTP 需要明确启用高风险选项。')
  }
  return url.toString().replace(/\/$/, '')
}

function normalizeRemotePath(value: string) {
  const clean = value.trim().replace(/^\/+/, '')
  if (!clean || clean.split('/').some((part) => part === '.' || part === '..')) throw new Error('WebDAV 远端路径无效')
  return clean
}

function sha256(value: string) {
  return createHash('sha256').update(value).digest('hex')
}
