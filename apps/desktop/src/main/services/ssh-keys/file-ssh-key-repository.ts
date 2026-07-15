import { chmod, mkdir, readFile, rename, rm, unlink, writeFile } from 'node:fs/promises'
import path from 'node:path'
import type { SshKeyRepository, StoredSshKey } from '@fileterm/storage'

type StoredSshKeyIndex = {
  version: 1
  keys: StoredSshKey[]
}

type StoredSshKeySecrets = {
  version: 1
  passphrases: Record<string, string>
}

export class FileSshKeyRepository implements SshKeyRepository {
  private readonly metadataPath: string
  private readonly secretsPath: string
  private readonly keysDirectory: string
  private readonly ready: Promise<void>

  constructor(baseDirectory: string) {
    this.metadataPath = path.join(baseDirectory, 'ssh-keys.json')
    this.secretsPath = path.join(baseDirectory, 'ssh-key-secrets.json')
    this.keysDirectory = path.join(baseDirectory, 'ssh-keys')
    this.ready = this.ensureStorage()
  }

  async list(): Promise<StoredSshKey[]> {
    await this.ready
    return [...(await this.readIndex()).keys]
  }

  async getById(id: string): Promise<StoredSshKey | null> {
    return (await this.list()).find((key) => key.id === id) ?? null
  }

  async getByFingerprint(fingerprint: string): Promise<StoredSshKey | null> {
    return (await this.list()).find((key) => key.fingerprint === fingerprint) ?? null
  }

  async create(key: StoredSshKey, privateKey: Uint8Array): Promise<StoredSshKey> {
    await this.ready
    if (await this.getById(key.id)) {
      throw new Error('SSH key already exists')
    }

    const keyPath = this.keyPath(key.id)
    const temporaryPath = `${keyPath}.${process.pid}.${Date.now()}.tmp`
    await writeFile(temporaryPath, privateKey)
    await lockDownFile(temporaryPath)

    try {
      await replaceFile(temporaryPath, keyPath)
      await this.writeIndex({
        version: 1,
        keys: [key, ...(await this.list())]
      })
      await lockDownFile(keyPath)
      return key
    } catch (error) {
      await rm(temporaryPath, { force: true }).catch(() => undefined)
      await rm(keyPath, { force: true }).catch(() => undefined)
      throw error
    }
  }

  async updateNote(id: string, note: string): Promise<StoredSshKey> {
    const index = await this.readIndex()
    const current = index.keys.find((key) => key.id === id)
    if (!current) {
      throw new Error('SSH key not found')
    }

    const updated = { ...current, note: note.trim() || undefined }
    await this.writeIndex({
      version: 1,
      keys: index.keys.map((key) => (key.id === id ? updated : key))
    })
    return updated
  }

  async delete(id: string): Promise<void> {
    const index = await this.readIndex()
    if (!index.keys.some((key) => key.id === id)) {
      return
    }

    await unlink(this.keyPath(id)).catch((error: NodeJS.ErrnoException) => {
      if (error.code !== 'ENOENT') throw error
    })
    await this.writeIndex({ version: 1, keys: index.keys.filter((key) => key.id !== id) })
    await this.setPassphrase(id, undefined)
  }

  async readPrivateKey(id: string): Promise<Uint8Array> {
    await this.ready
    if (!(await this.getById(id))) {
      throw new Error('SSH key not found')
    }
    return readFile(this.keyPath(id))
  }

  async getPassphrase(id: string): Promise<string | undefined> {
    await this.ready
    return (await this.readSecrets()).passphrases[id]
  }

  async setPassphrase(id: string, passphrase: string | undefined): Promise<void> {
    await this.ready
    const secrets = await this.readSecrets()
    const passphrases = { ...secrets.passphrases }
    if (passphrase) {
      passphrases[id] = passphrase
    } else {
      delete passphrases[id]
    }
    await writeJsonAtomic(this.secretsPath, { version: 1, passphrases } satisfies StoredSshKeySecrets)
    await lockDownFile(this.secretsPath)
  }

  private keyPath(id: string) {
    if (!/^[a-f0-9-]{36}$/i.test(id)) {
      throw new Error('Invalid SSH key id')
    }
    return path.join(this.keysDirectory, `${id}.key`)
  }

  private async ensureStorage() {
    await mkdir(this.keysDirectory, { recursive: true })
    await lockDownDirectory(this.keysDirectory)
    await ensureJsonFile(this.metadataPath, { version: 1, keys: [] } satisfies StoredSshKeyIndex)
    await ensureJsonFile(this.secretsPath, { version: 1, passphrases: {} } satisfies StoredSshKeySecrets)
    await lockDownFile(this.secretsPath)
  }

  private async readIndex(): Promise<StoredSshKeyIndex> {
    await this.ready
    return readJson(this.metadataPath, { version: 1, keys: [] })
  }

  private async writeIndex(index: StoredSshKeyIndex) {
    await writeJsonAtomic(this.metadataPath, index)
  }

  private async readSecrets(): Promise<StoredSshKeySecrets> {
    return readJson(this.secretsPath, { version: 1, passphrases: {} })
  }
}

async function ensureJsonFile<T>(filePath: string, fallback: T) {
  try {
    await readFile(filePath, 'utf8')
  } catch {
    await writeJsonAtomic(filePath, fallback)
  }
}

async function readJson<T>(filePath: string, fallback: T): Promise<T> {
  try {
    return JSON.parse(await readFile(filePath, 'utf8')) as T
  } catch {
    return fallback
  }
}

async function writeJsonAtomic(filePath: string, value: unknown) {
  await mkdir(path.dirname(filePath), { recursive: true })
  const temporaryPath = `${filePath}.${process.pid}.${Date.now()}.tmp`
  await writeFile(temporaryPath, JSON.stringify(value, null, 2), 'utf8')
  await replaceFile(temporaryPath, filePath)
}

async function replaceFile(sourcePath: string, destinationPath: string) {
  try {
    await rename(sourcePath, destinationPath)
  } catch (error) {
    const errno = error as NodeJS.ErrnoException
    if (process.platform !== 'win32' || (errno.code !== 'EEXIST' && errno.code !== 'EPERM')) {
      throw error
    }
    await rm(destinationPath, { force: true })
    await rename(sourcePath, destinationPath)
  }
}

async function lockDownFile(filePath: string) {
  await chmod(filePath, 0o600).catch(() => undefined)
}

async function lockDownDirectory(directoryPath: string) {
  await chmod(directoryPath, 0o700).catch(() => undefined)
}
