import { createHash, randomUUID } from 'node:crypto'
import { readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import { dialog, type BrowserWindow, type OpenDialogOptions } from 'electron'
import ssh2 from 'ssh2'
import type { ImportSshKeyInput, SshKeyImportResult, SshKeyMetadata } from '@fileterm/core'
import type { ProfileRepository, SshKeyRepository, StoredSshKey } from '@fileterm/storage'

const { utils } = ssh2
const MAX_PRIVATE_KEY_BYTES = 1024 * 1024

export type ResolvedSshKey = {
  key: StoredSshKey
  privateKey: Buffer
  savedPassphrase?: string
}

export class SshKeyService {
  private readonly repository: SshKeyRepository
  private readonly profiles: ProfileRepository

  constructor(repository: SshKeyRepository, profiles: ProfileRepository) {
    this.repository = repository
    this.profiles = profiles
  }

  async list(): Promise<SshKeyMetadata[]> {
    const [keys, profiles] = await Promise.all([this.repository.list(), this.profiles.list()])
    return keys.map((key) => ({
      ...key,
      usageCount: profiles.filter((profile) => profile.type === 'ssh' && profile.privateKeyId === key.id).length
    }))
  }

  async import(input: ImportSshKeyInput = {}, parent?: BrowserWindow): Promise<SshKeyImportResult | null> {
    const sourcePath = input.sourcePath ?? (await this.selectPrivateKey(parent))
    if (!sourcePath) {
      return null
    }

    const fileStat = await stat(sourcePath)
    if (!fileStat.isFile()) {
      throw new Error('请选择有效的私钥文件。')
    }
    if (fileStat.size <= 0 || fileStat.size > MAX_PRIVATE_KEY_BYTES) {
      throw new Error('私钥文件为空或超过 1 MB 限制。')
    }

    const privateKey = await readFile(sourcePath)
    const inspected = inspectPrivateKey(privateKey)
    const existing = await this.repository.getByFingerprint(inspected.fingerprint)
    if (existing) {
      return {
        key: await this.toMetadata(existing),
        duplicate: true
      }
    }

    const key: StoredSshKey = {
      id: randomUUID(),
      name: path.basename(sourcePath),
      note: input.note?.trim() || undefined,
      algorithm: inspected.algorithm,
      fingerprint: inspected.fingerprint,
      encrypted: inspected.encrypted,
      importedAt: Date.now()
    }
    await this.repository.create(key, privateKey)
    return {
      key: { ...key, usageCount: 0 },
      duplicate: false
    }
  }

  async updateNote(keyId: string, note: string): Promise<SshKeyMetadata> {
    return this.toMetadata(await this.repository.updateNote(keyId, note))
  }

  async delete(keyId: string): Promise<void> {
    const profiles = (await this.profiles.list()).filter(
      (profile) => profile.type === 'ssh' && profile.privateKeyId === keyId
    )
    if (profiles.length > 0) {
      throw new Error(`该密钥正在被以下连接使用：${profiles.map((profile) => profile.name).join('、')}`)
    }
    await this.repository.delete(keyId)
  }

  async resolve(keyId: string): Promise<ResolvedSshKey> {
    const key = await this.repository.getById(keyId)
    if (!key) {
      throw new Error('选择的 SSH 密钥不存在，请重新选择。')
    }
    return {
      key,
      privateKey: Buffer.from(await this.repository.readPrivateKey(keyId)),
      savedPassphrase: await this.repository.getPassphrase(keyId)
    }
  }

  async validatePassphrase(keyId: string, passphrase: string): Promise<void> {
    const resolved = await this.resolve(keyId)
    const parsed = utils.parseKey(resolved.privateKey, passphrase)
    if (parsed instanceof Error || !parsed.isPrivateKey()) {
      throw new Error('私钥口令不正确。')
    }
  }

  setPassphrase(keyId: string, passphrase: string | undefined): Promise<void> {
    return this.repository.setPassphrase(keyId, passphrase)
  }

  private async toMetadata(key: StoredSshKey): Promise<SshKeyMetadata> {
    const profiles = await this.profiles.list()
    return {
      ...key,
      usageCount: profiles.filter((profile) => profile.type === 'ssh' && profile.privateKeyId === key.id).length
    }
  }

  private async selectPrivateKey(parent?: BrowserWindow): Promise<string | null> {
    const result = parent
      ? await dialog.showOpenDialog(parent, privateKeyDialogOptions())
      : await dialog.showOpenDialog(privateKeyDialogOptions())
    return result.canceled ? null : (result.filePaths[0] ?? null)
  }
}

function privateKeyDialogOptions(): OpenDialogOptions {
  return {
    title: '导入 SSH 私钥',
    properties: ['openFile'],
    filters: [
      { name: 'SSH 私钥', extensions: ['pem', 'key', 'ppk', 'openssh'] },
      { name: '所有文件', extensions: ['*'] }
    ]
  }
}

function inspectPrivateKey(privateKey: Buffer): {
  algorithm: string
  fingerprint: string
  encrypted: boolean
} {
  const parsed = utils.parseKey(privateKey)
  if (!(parsed instanceof Error)) {
    if (!parsed.isPrivateKey()) {
      throw new Error('选择的文件是公钥，不是 SSH 私钥。')
    }
    return {
      algorithm: parsed.type,
      fingerprint: publicKeyFingerprint(parsed.getPublicSSH()),
      encrypted: false
    }
  }

  if (isEncryptedPrivateKeyError(parsed)) {
    return {
      algorithm: 'encrypted',
      fingerprint: contentFingerprint(privateKey),
      encrypted: true
    }
  }

  throw new Error('无法识别该文件，请选择 OpenSSH、PEM 或兼容格式的私钥。')
}

function isEncryptedPrivateKeyError(error: Error) {
  return /encrypted|passphrase/i.test(error.message)
}

function publicKeyFingerprint(publicKey: Buffer) {
  return `SHA256:${createHash('sha256').update(publicKey).digest('base64').replace(/=+$/, '')}`
}

function contentFingerprint(privateKey: Buffer) {
  return `FILE-SHA256:${createHash('sha256').update(privateKey).digest('base64url')}`
}
