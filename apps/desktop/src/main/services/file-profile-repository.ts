import { chmod, mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { randomUUID } from 'node:crypto'
import { safeStorage } from 'electron'
import type {
  CommandSendPreferences,
  CommandFolder,
  TerminalCommandHistoryEntry,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  CreateProfileInput
} from '@termdock/core'
import { normalizeConnectionHost, validateConnectionHost } from '@termdock/shared'
import type { ProfileRepository } from '@termdock/storage'

const legacyDemoProfileIds = new Set([
  'profile-ssh-prod',
  'profile-ssh-nas',
  'profile-ftp-archive'
])

const legacyDemoCommandFolderIds = new Set([
  'cmd-folder-default',
  'cmd-folder-deploy'
])

const legacyDemoCommandTemplateIds = new Set([
  'cmd-docker-ps',
  'cmd-tail-log',
  'cmd-restart-service'
])

type ProfileSecretField = 'password' | 'privateKeyPath' | 'passphrase'

type StoredProfileSecret = {
  storage: 'safeStorage' | 'plain-text-fallback'
  value: string
}

type StoredProfileSecrets = {
  version: 1
  profiles: Record<string, Partial<Record<ProfileSecretField, StoredProfileSecret>>>
}

export class FileProfileRepository implements ProfileRepository {
  private readonly filePath: string
  private readonly secretsPath: string
  private readonly foldersPath: string
  private readonly commandFoldersPath: string
  private readonly commandsPath: string
  private readonly commandHistoryPath: string
  private readonly commandSendPreferencesPath: string
  private readonly seedProfiles: ConnectionProfile[]
  private readonly seedCommandTemplates: CommandTemplate[]
  private readonly seedCommandFolders: CommandFolder[]
  private ready: Promise<void>

  constructor(
    baseDir: string,
    seedProfiles: ConnectionProfile[],
    seedCommandTemplates: CommandTemplate[] = [],
    seedCommandFolders: CommandFolder[] = []
  ) {
    this.filePath = path.join(baseDir, 'profiles.json')
    this.secretsPath = path.join(baseDir, 'profile-secrets.json')
    this.foldersPath = path.join(baseDir, 'folders.json')
    this.commandFoldersPath = path.join(baseDir, 'command-folders.json')
    this.commandsPath = path.join(baseDir, 'commands.json')
    this.commandHistoryPath = path.join(baseDir, 'command-history.json')
    this.commandSendPreferencesPath = path.join(baseDir, 'command-send-preferences.json')
    this.seedProfiles = seedProfiles
    this.seedCommandTemplates = seedCommandTemplates
    this.seedCommandFolders = seedCommandFolders
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
    const previous = profiles.find((item) => item.id === id)
    if (!previous) {
      throw new Error('Profile not found')
    }
    const profile = preserveProfileMetadata(toProfile(id, input), previous)
    const nextProfiles = profiles.map((item) => (item.id === id ? profile : item))
    await this.writeProfiles(nextProfiles)
    return profile
  }

  async getById(id: string): Promise<ConnectionProfile | null> {
    const profiles = await this.readProfiles()
    return profiles.find((profile) => profile.id === id) ?? null
  }

  async updateTrustedHostFingerprint(id: string, fingerprint: string): Promise<ConnectionProfile | null> {
    const profiles = await this.readProfiles()
    let updatedProfile: ConnectionProfile | null = null
    const nextProfiles = profiles.map((profile) => {
      if (profile.id !== id || profile.type !== 'ssh') {
        return profile
      }

      updatedProfile = {
        ...profile,
        trustedHostFingerprint: fingerprint
      }
      return updatedProfile
    })

    if (!updatedProfile) {
      return null
    }

    await this.writeProfiles(nextProfiles)
    return updatedProfile
  }

  async delete(id: string): Promise<void> {
    const profiles = await this.readProfiles()
    const nextProfiles = profiles.filter((profile) => profile.id !== id)
    await this.writeProfiles(nextProfiles)
  }

  async touchProfile(id: string): Promise<void> {
    const profiles = await this.readProfiles()
    const now = Date.now()
    const nextProfiles = profiles.map((profile) =>
      profile.id === id ? { ...profile, lastUsedAt: now } : profile
    )
    await this.writeProfiles(nextProfiles)
  }

  async listFolders(): Promise<ConnectionFolder[]> {
    const folders = await this.readFolders()
    return [...folders]
  }

  async createFolder(name: string, parentId?: string): Promise<ConnectionFolder> {
    const folders = await this.readFolders()
    const folder: ConnectionFolder = {
      id: randomUUID(),
      type: 'folder',
      name,
      parentId,
      order: Date.now()
    }
    await this.writeFolders([folder, ...folders])
    return folder
  }

  async updateFolder(id: string, updates: Partial<ConnectionFolder>): Promise<ConnectionFolder> {
    const folders = await this.readFolders()
    let updatedFolder: ConnectionFolder | undefined
    const nextFolders = folders.map((f) => {
      if (f.id === id) {
        updatedFolder = { ...f, ...updates }
        return updatedFolder
      }
      return f
    })
    if (!updatedFolder) throw new Error('Folder not found')
    await this.writeFolders(nextFolders)
    return updatedFolder
  }

  async deleteFolder(id: string): Promise<void> {
    const [profiles, folders] = await Promise.all([
      this.readProfiles(),
      this.readFolders()
    ])
    const folder = folders.find((item) => item.id === id)
    if (!folder) {
      return
    }
    const nextParentId = folder.parentId
    await Promise.all([
      this.writeProfiles(profiles.map((profile) => (
        profile.parentId === id ? { ...profile, parentId: nextParentId } : profile
      ))),
      this.writeFolders(folders
        .filter((item) => item.id !== id)
        .map((item) => (
          item.parentId === id ? { ...item, parentId: nextParentId } : item
        )))
    ])
  }

  async updateOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void> {
    const profiles = await this.readProfiles()
    let found = false
    const nextProfiles = profiles.map((p) => {
      if (p.id === id) {
        found = true
        return { ...p, parentId: newParentId, order: newOrder }
      }
      return p
    })
    if (found) {
      await this.writeProfiles(nextProfiles)
      return
    }

    const folders = await this.readFolders()
    const nextFolders = folders.map((f) => {
      if (f.id === id) {
        return { ...f, parentId: newParentId, order: newOrder }
      }
      return f
    })
    await this.writeFolders(nextFolders)
  }

  async listCommandFolders(): Promise<CommandFolder[]> {
    const folders = await this.readCommandFolders()
    return [...folders]
  }

  async createCommandFolder(name: string, parentId?: string): Promise<CommandFolder> {
    const folders = await this.readCommandFolders()
    const folder: CommandFolder = {
      id: randomUUID(),
      type: 'command-folder',
      name,
      parentId,
      order: Date.now()
    }
    await this.writeCommandFolders([folder, ...folders])
    return folder
  }

  async updateCommandFolder(id: string, updates: Partial<CommandFolder>): Promise<CommandFolder> {
    const folders = await this.readCommandFolders()
    let updatedFolder: CommandFolder | undefined
    const nextFolders = folders.map((item) => {
      if (item.id === id) {
        updatedFolder = { ...item, ...updates }
        return updatedFolder
      }
      return item
    })
    if (!updatedFolder) throw new Error('Command folder not found')
    await this.writeCommandFolders(nextFolders)
    return updatedFolder
  }

  async deleteCommandFolder(id: string): Promise<void> {
    const [folders, commands] = await Promise.all([
      this.readCommandFolders(),
      this.readCommandTemplates()
    ])
    const folder = folders.find((item) => item.id === id)
    if (!folder) {
      return
    }
    const nextParentId = folder.parentId
    await Promise.all([
      this.writeCommandFolders(folders
        .filter((item) => item.id !== id)
        .map((item) => (
          item.parentId === id ? { ...item, parentId: nextParentId } : item
        ))),
      this.writeCommandTemplates(commands.map((item) => (
        item.parentId === id ? { ...item, parentId: nextParentId } : item
      )))
    ])
  }

  async listCommandTemplates(): Promise<CommandTemplate[]> {
    const commands = await this.readCommandTemplates()
    return [...commands]
  }

  async createCommandTemplate(input: CommandTemplateInput): Promise<CommandTemplate> {
    const commands = await this.readCommandTemplates()
    const command = toCommandTemplate(randomUUID(), input)
    await this.writeCommandTemplates([command, ...commands])
    return command
  }

  async updateCommandTemplate(id: string, input: CommandTemplateInput): Promise<CommandTemplate> {
    const commands = await this.readCommandTemplates()
    const command = toCommandTemplate(id, input)
    const nextCommands = commands.map((item) => (item.id === id ? command : item))
    await this.writeCommandTemplates(nextCommands)
    return command
  }

  async getCommandTemplateById(id: string): Promise<CommandTemplate | null> {
    const commands = await this.readCommandTemplates()
    return commands.find((item) => item.id === id) ?? null
  }

  async deleteCommandTemplate(id: string): Promise<void> {
    const commands = await this.readCommandTemplates()
    await this.writeCommandTemplates(commands.filter((item) => item.id !== id))
  }

  async updateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void> {
    const [folders, commands] = await Promise.all([
      this.readCommandFolders(),
      this.readCommandTemplates()
    ])
    const folderFound = folders.some((item) => item.id === id)
    if (folderFound) {
      await this.writeCommandFolders(folders.map((item) => (
        item.id === id ? { ...item, parentId: newParentId, order: newOrder } : item
      )))
      return
    }

    await this.writeCommandTemplates(commands.map((item) => (
      item.id === id ? { ...item, parentId: newParentId, order: newOrder } : item
    )))
  }

  async getTerminalCommandHistory(profileId: string): Promise<TerminalCommandHistoryEntry[]> {
    const historyMap = await this.readCommandHistoryMap()
    return [...(historyMap[profileId] ?? [])]
  }

  async setTerminalCommandHistory(profileId: string, entries: TerminalCommandHistoryEntry[]): Promise<void> {
    const historyMap = await this.readCommandHistoryMap()
    const nextEntries = entries
      .filter((entry) => entry.command.trim().length > 0 && Number.isFinite(entry.createdAt))
      .map((entry) => ({
        command: entry.command,
        createdAt: entry.createdAt
      }))

    await this.writeCommandHistoryMap({
      ...historyMap,
      [profileId]: nextEntries
    })
  }

  async getCommandSendPreferences(): Promise<CommandSendPreferences> {
    return this.readCommandSendPreferences()
  }

  async setCommandSendPreferences(preferences: CommandSendPreferences): Promise<void> {
    await this.writeCommandSendPreferences({
      rememberSelection: preferences.rememberSelection,
      sendScope: preferences.sendScope,
      selectedTabIds: preferences.selectedTabIds
    })
  }

  private async ensureFile() {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    try {
      await readFile(this.filePath, 'utf8')
    } catch {
      await this.writeProfiles(this.seedProfiles)
    }
    try {
      await readFile(this.foldersPath, 'utf8')
    } catch {
      await this.writeFolders([])
    }
    try {
      await readFile(this.commandFoldersPath, 'utf8')
    } catch {
      await this.writeCommandFolders(this.seedCommandFolders)
    }
    try {
      await readFile(this.commandsPath, 'utf8')
    } catch {
      await this.writeCommandTemplates(this.seedCommandTemplates)
    }
    try {
      await readFile(this.commandHistoryPath, 'utf8')
    } catch {
      await this.writeCommandHistoryMap({})
    }
    try {
      await readFile(this.commandSendPreferencesPath, 'utf8')
    } catch {
      await this.writeCommandSendPreferences({
        rememberSelection: false,
        sendScope: 'current',
        selectedTabIds: []
      })
    }

    await this.removeLegacyDemoData()
    await this.migrateProfileSecrets()
  }

  private async removeLegacyDemoData() {
    const [profiles, commandFolders, commandTemplates] = await Promise.all([
      readJsonFile<ConnectionProfile[]>(this.filePath, []),
      readJsonFile<CommandFolder[]>(this.commandFoldersPath, []),
      readJsonFile<CommandTemplate[]>(this.commandsPath, [])
    ])

    const nextProfiles = profiles.filter((profile) => !legacyDemoProfileIds.has(profile.id))
    const nextCommandFolders = commandFolders.filter((folder) => !legacyDemoCommandFolderIds.has(folder.id))
    const nextCommandTemplates = commandTemplates
      .filter((command) => !legacyDemoCommandTemplateIds.has(command.id))
      .map((command) => (
        command.parentId && legacyDemoCommandFolderIds.has(command.parentId)
          ? { ...command, parentId: undefined }
          : command
      ))

    await Promise.all([
      nextProfiles.length === profiles.length ? undefined : this.writeProfiles(nextProfiles),
      nextCommandFolders.length === commandFolders.length ? undefined : this.writeCommandFolders(nextCommandFolders),
      nextCommandTemplates.length === commandTemplates.length
        && nextCommandTemplates.every((command, index) => command.parentId === commandTemplates[index]?.parentId)
        ? undefined
        : this.writeCommandTemplates(nextCommandTemplates)
    ])
  }

  private async readCommandHistoryMap(): Promise<Record<string, TerminalCommandHistoryEntry[]>> {
    await this.ready
    return readJsonFile<Record<string, TerminalCommandHistoryEntry[]>>(this.commandHistoryPath, {})
  }

  private async writeCommandHistoryMap(historyMap: Record<string, TerminalCommandHistoryEntry[]>) {
    await mkdir(path.dirname(this.commandHistoryPath), { recursive: true })
    await writeFile(this.commandHistoryPath, JSON.stringify(historyMap, null, 2), 'utf8')
  }

  private async readCommandSendPreferences(): Promise<CommandSendPreferences> {
    await this.ready
    return readJsonFile<CommandSendPreferences>(this.commandSendPreferencesPath, {
      rememberSelection: false,
      sendScope: 'current',
      selectedTabIds: []
    })
  }

  private async writeCommandSendPreferences(preferences: CommandSendPreferences) {
    await mkdir(path.dirname(this.commandSendPreferencesPath), { recursive: true })
    await writeFile(this.commandSendPreferencesPath, JSON.stringify(preferences, null, 2), 'utf8')
  }

  private async readProfiles(): Promise<ConnectionProfile[]> {
    await this.ready
    const content = await readFile(this.filePath, 'utf8')
    const profiles = JSON.parse(content) as ConnectionProfile[]
    const secrets = await this.readProfileSecrets()
    return profiles.map((profile) => mergeProfileSecrets(profile, secrets.profiles[profile.id]))
  }

  private async writeProfiles(profiles: ConnectionProfile[]) {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    const { publicProfiles, secrets } = splitProfileSecrets(profiles)
    await Promise.all([
      writeFile(this.filePath, JSON.stringify(publicProfiles, null, 2), 'utf8'),
      writeFile(this.secretsPath, JSON.stringify(secrets, null, 2), 'utf8')
    ])
    await lockDownFile(this.secretsPath)
  }

  private async migrateProfileSecrets() {
    const profiles = await readJsonFile<ConnectionProfile[]>(this.filePath, [])
    if (!profiles.some(hasInlineProfileSecret)) {
      return
    }
    await this.writeProfiles(profiles)
  }

  private async readProfileSecrets(): Promise<StoredProfileSecrets> {
    return readJsonFile<StoredProfileSecrets>(this.secretsPath, createEmptyProfileSecrets())
  }

  private async readFolders(): Promise<ConnectionFolder[]> {
    await this.ready
    const content = await readFile(this.foldersPath, 'utf8')
    return JSON.parse(content) as ConnectionFolder[]
  }

  private async writeFolders(folders: ConnectionFolder[]) {
    await mkdir(path.dirname(this.foldersPath), { recursive: true })
    await writeFile(this.foldersPath, JSON.stringify(folders, null, 2), 'utf8')
  }

  private async readCommandFolders(): Promise<CommandFolder[]> {
    await this.ready
    const content = await readFile(this.commandFoldersPath, 'utf8')
    return JSON.parse(content) as CommandFolder[]
  }

  private async writeCommandFolders(folders: CommandFolder[]) {
    await mkdir(path.dirname(this.commandFoldersPath), { recursive: true })
    await writeFile(this.commandFoldersPath, JSON.stringify(folders, null, 2), 'utf8')
  }

  private async readCommandTemplates(): Promise<CommandTemplate[]> {
    await this.ready
    const content = await readFile(this.commandsPath, 'utf8')
    return JSON.parse(content) as CommandTemplate[]
  }

  private async writeCommandTemplates(commands: CommandTemplate[]) {
    await mkdir(path.dirname(this.commandsPath), { recursive: true })
    await writeFile(this.commandsPath, JSON.stringify(commands, null, 2), 'utf8')
  }
}

async function readJsonFile<T>(filePath: string, fallback: T): Promise<T> {
  try {
    return JSON.parse(await readFile(filePath, 'utf8')) as T
  } catch {
    return fallback
  }
}

function createEmptyProfileSecrets(): StoredProfileSecrets {
  return {
    version: 1,
    profiles: {}
  }
}

function preserveProfileMetadata(profile: ConnectionProfile, previous: ConnectionProfile): ConnectionProfile {
  return {
    ...profile,
    parentId: previous.parentId,
    order: previous.order,
    lastUsedAt: previous.lastUsedAt
  }
}

function hasInlineProfileSecret(profile: ConnectionProfile) {
  return Boolean(
    profile.password
      || (profile.type === 'ssh' && (profile.privateKeyPath || profile.passphrase))
  )
}

function splitProfileSecrets(profiles: ConnectionProfile[]) {
  const secrets = createEmptyProfileSecrets()
  const publicProfiles = profiles.map((profile) => {
    const profileSecrets = extractProfileSecrets(profile)
    if (Object.keys(profileSecrets).length > 0) {
      secrets.profiles[profile.id] = profileSecrets
    }
    return stripProfileSecrets(profile)
  })

  return { publicProfiles, secrets }
}

function extractProfileSecrets(profile: ConnectionProfile): Partial<Record<ProfileSecretField, StoredProfileSecret>> {
  const secrets: Partial<Record<ProfileSecretField, StoredProfileSecret>> = {}
  for (const field of getProfileSecretFields(profile)) {
    const value = getProfileSecretValue(profile, field)
    if (value) {
      secrets[field] = encodeProfileSecret(value)
    }
  }
  return secrets
}

function getProfileSecretFields(profile: ConnectionProfile): ProfileSecretField[] {
  return profile.type === 'ssh'
    ? ['password', 'privateKeyPath', 'passphrase']
    : ['password']
}

function getProfileSecretValue(profile: ConnectionProfile, field: ProfileSecretField) {
  if (field === 'password') {
    return profile.password
  }
  if (profile.type !== 'ssh') {
    return undefined
  }
  return profile[field]
}

function encodeProfileSecret(value: string): StoredProfileSecret {
  if (safeStorage.isEncryptionAvailable()) {
    return {
      storage: 'safeStorage',
      value: safeStorage.encryptString(value).toString('base64')
    }
  }

  return {
    storage: 'plain-text-fallback',
    value
  }
}

function decodeProfileSecret(secret: StoredProfileSecret | undefined) {
  if (!secret) {
    return undefined
  }
  if (secret.storage === 'plain-text-fallback') {
    return secret.value
  }
  if (!safeStorage.isEncryptionAvailable()) {
    return undefined
  }
  try {
    return safeStorage.decryptString(Buffer.from(secret.value, 'base64'))
  } catch {
    return undefined
  }
}

function stripProfileSecrets(profile: ConnectionProfile): ConnectionProfile {
  if (profile.type === 'ssh') {
    const { password, privateKeyPath, passphrase, ...publicProfile } = profile
    return publicProfile
  }

  const { password, ...publicProfile } = profile
  return publicProfile
}

function mergeProfileSecrets(
  profile: ConnectionProfile,
  storedSecrets: Partial<Record<ProfileSecretField, StoredProfileSecret>> | undefined
): ConnectionProfile {
  if (!storedSecrets) {
    return profile
  }

  let next = profile
  for (const field of getProfileSecretFields(profile)) {
    const value = decodeProfileSecret(storedSecrets[field])
    if (!value) {
      continue
    }
    next = withProfileSecretValue(next, field, value)
  }
  return next
}

function withProfileSecretValue(
  profile: ConnectionProfile,
  field: ProfileSecretField,
  value: string
): ConnectionProfile {
  if (field === 'password') {
    return { ...profile, password: value }
  }
  if (profile.type !== 'ssh') {
    return profile
  }
  return { ...profile, [field]: value }
}

async function lockDownFile(filePath: string) {
  try {
    await chmod(filePath, 0o600)
  } catch {
    // Best effort only: chmod is not equally meaningful on every target platform.
  }
}

function toProfile(id: string, input: CreateProfileInput): ConnectionProfile {
  const host = normalizeConnectionHost(input.host)
  if (!validateConnectionHost(host).valid) {
    throw new Error('Invalid host')
  }

  return input.type === 'ssh'
    ? {
        id,
        type: 'ssh',
        name: input.name,
        host,
        port: input.port,
        username: input.username,
        authType: input.authType ?? 'system',
        note: input.note,
        password: input.password,
        privateKeyPath: input.privateKeyPath,
        passphrase: input.passphrase,
        trustedHostFingerprint: input.trustedHostFingerprint,
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
        host,
        port: input.port,
        username: input.username,
        note: input.note,
        password: input.password,
        secure: input.secure ?? false,
        group: input.group,
        remotePath: input.remotePath
      }
}

function toCommandTemplate(id: string, input: CommandTemplateInput): CommandTemplate {
  return {
    id,
    type: 'command-template',
    name: input.name,
    command: input.command,
    description: input.description,
    parentId: input.parentId,
    order: input.order ?? Date.now(),
    appendCarriageReturn: input.appendCarriageReturn ?? true
  }
}
