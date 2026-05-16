import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { randomUUID } from 'node:crypto'
import type { ConnectionProfile, CreateProfileInput } from '@termdock/core'
import type { ProfileRepository } from '@termdock/storage'

export class FileProfileRepository implements ProfileRepository {
  private readonly filePath: string
  private readonly seedProfiles: ConnectionProfile[]
  private ready: Promise<void>

  constructor(baseDir: string, seedProfiles: ConnectionProfile[]) {
    this.filePath = path.join(baseDir, 'profiles.json')
    this.seedProfiles = seedProfiles
    this.ready = this.ensureFile()
  }

  async list(): Promise<ConnectionProfile[]> {
    const profiles = await this.readProfiles()
    return [...profiles]
  }

  async create(input: CreateProfileInput): Promise<ConnectionProfile> {
    const profiles = await this.readProfiles()
    const profile = toProfile(randomUUID(), input)

    const nextProfiles = [profile, ...profiles]
    await this.writeProfiles(nextProfiles)
    return profile
  }

  async update(id: string, input: CreateProfileInput): Promise<ConnectionProfile> {
    const profiles = await this.readProfiles()
    const profile = toProfile(id, input)
    const nextProfiles = profiles.map((item) => (item.id === id ? profile : item))
    await this.writeProfiles(nextProfiles)
    return profile
  }

  async getById(id: string): Promise<ConnectionProfile | null> {
    const profiles = await this.readProfiles()
    return profiles.find((profile) => profile.id === id) ?? null
  }

  async delete(id: string): Promise<void> {
    const profiles = await this.readProfiles()
    const nextProfiles = profiles.filter((profile) => profile.id !== id)
    await this.writeProfiles(nextProfiles)
  }

  private async ensureFile() {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    try {
      await readFile(this.filePath, 'utf8')
    } catch {
      await this.writeProfiles(this.seedProfiles)
    }
  }

  private async readProfiles(): Promise<ConnectionProfile[]> {
    await this.ready
    const content = await readFile(this.filePath, 'utf8')
    return JSON.parse(content) as ConnectionProfile[]
  }

  private async writeProfiles(profiles: ConnectionProfile[]) {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    await writeFile(this.filePath, JSON.stringify(profiles, null, 2), 'utf8')
  }
}

function toProfile(id: string, input: CreateProfileInput): ConnectionProfile {
  return input.type === 'ssh'
    ? {
        id,
        type: 'ssh',
        name: input.name,
        host: input.host,
        port: input.port,
        username: input.username,
        authType: input.authType ?? 'password',
        note: input.note,
        password: input.password,
        privateKeyPath: input.privateKeyPath,
        passphrase: input.passphrase,
        group: input.group,
        sftpEnabled: true,
        remotePath: input.remotePath,
        encoding: input.encoding ?? 'UTF-8',
        backspaceKey: input.backspaceKey ?? 'ASCII',
        deleteKey: input.deleteKey ?? 'VT220',
        enableExecChannel: input.enableExecChannel ?? true
      }
    : {
        id,
        type: 'ftp',
        name: input.name,
        host: input.host,
        port: input.port,
        username: input.username,
        note: input.note,
        password: input.password,
        secure: input.secure ?? false,
        group: input.group,
        remotePath: input.remotePath
      }
}
