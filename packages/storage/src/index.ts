import type {
  CommandSendPreferences,
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  CreateProfileInput,
  TerminalCommandHistoryEntry
} from '@fileterm/core'

export interface ProfileRepository {
  list(): Promise<ConnectionProfile[]>
  listFolders(): Promise<ConnectionFolder[]>
  create(input: CreateProfileInput): Promise<ConnectionProfile>
  update(id: string, input: CreateProfileInput): Promise<ConnectionProfile>
  updateTrustedHostFingerprint?(id: string, fingerprint: string): Promise<ConnectionProfile | null>
  getById(id: string): Promise<ConnectionProfile | null>
  delete(id: string): Promise<void>
  touchProfile(id: string): Promise<void>
  
  createFolder(name: string, parentId?: string): Promise<ConnectionFolder>
  updateFolder(id: string, updates: Partial<ConnectionFolder>): Promise<ConnectionFolder>
  deleteFolder(id: string): Promise<void>
  
  updateOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void>

  listCommandFolders(): Promise<CommandFolder[]>
  createCommandFolder(name: string, parentId?: string): Promise<CommandFolder>
  updateCommandFolder(id: string, updates: Partial<CommandFolder>): Promise<CommandFolder>
  deleteCommandFolder(id: string): Promise<void>
  listCommandTemplates(): Promise<CommandTemplate[]>
  createCommandTemplate(input: CommandTemplateInput): Promise<CommandTemplate>
  updateCommandTemplate(id: string, input: CommandTemplateInput): Promise<CommandTemplate>
  getCommandTemplateById(id: string): Promise<CommandTemplate | null>
  deleteCommandTemplate(id: string): Promise<void>
  updateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void>
  getTerminalCommandHistory(profileId: string): Promise<TerminalCommandHistoryEntry[]>
  setTerminalCommandHistory(profileId: string, entries: TerminalCommandHistoryEntry[]): Promise<void>
  getCommandSendPreferences(): Promise<CommandSendPreferences>
  setCommandSendPreferences(preferences: CommandSendPreferences): Promise<void>
}

export class MemoryProfileRepository implements ProfileRepository {
  private profiles: ConnectionProfile[]
  private folders: ConnectionFolder[] = []
  private commandFolders: CommandFolder[] = []
  private commandTemplates: CommandTemplate[] = []

  constructor(seed: ConnectionProfile[]) {
    this.profiles = seed
  }

  async list(): Promise<ConnectionProfile[]> {
    return [...this.profiles]
  }

  async create(input: CreateProfileInput): Promise<ConnectionProfile> {
    const id = globalThis.crypto?.randomUUID?.() ?? `profile-${Date.now()}`
    const profile = toProfile(id, input)

    this.profiles = [profile, ...this.profiles]
    return profile
  }

  async update(id: string, input: CreateProfileInput): Promise<ConnectionProfile> {
    const previous = this.profiles.find((item) => item.id === id)
    if (!previous) {
      throw new Error('Profile not found')
    }
    const profile = preserveProfileMetadata(toProfile(id, input), previous)
    this.profiles = this.profiles.map((item) => (item.id === id ? profile : item))
    return profile
  }

  async getById(id: string): Promise<ConnectionProfile | null> {
    return this.profiles.find((profile) => profile.id === id) ?? null
  }

  async updateTrustedHostFingerprint(id: string, fingerprint: string): Promise<ConnectionProfile | null> {
    let updatedProfile: ConnectionProfile | null = null
    this.profiles = this.profiles.map((profile) => {
      if (profile.id !== id || profile.type !== 'ssh') {
        return profile
      }

      updatedProfile = {
        ...profile,
        trustedHostFingerprint: fingerprint
      }
      return updatedProfile
    })

    return updatedProfile
  }

  async delete(id: string): Promise<void> {
    this.profiles = this.profiles.filter((profile) => profile.id !== id)
  }

  async touchProfile(id: string): Promise<void> {
    const now = Date.now()
    this.profiles = this.profiles.map((profile) =>
      profile.id === id ? { ...profile, lastUsedAt: now } : profile
    )
  }

  async listFolders(): Promise<ConnectionFolder[]> {
    return [...this.folders]
  }

  async createFolder(name: string, parentId?: string): Promise<ConnectionFolder> {
    const id = globalThis.crypto?.randomUUID?.() ?? `folder-${Date.now()}`
    const folder: ConnectionFolder = { id, type: 'folder', name, parentId }
    this.folders.push(folder)
    return folder
  }

  async updateFolder(id: string, updates: Partial<ConnectionFolder>): Promise<ConnectionFolder> {
    const folder = this.folders.find((f) => f.id === id)
    if (!folder) throw new Error('Folder not found')
    Object.assign(folder, updates)
    return folder
  }

  async deleteFolder(id: string): Promise<void> {
    const folder = this.folders.find((item) => item.id === id)
    if (!folder) {
      return
    }
    const nextParentId = folder.parentId
    this.profiles = this.profiles.map((profile) => (
      profile.parentId === id ? { ...profile, parentId: nextParentId } : profile
    ))
    this.folders = this.folders
      .filter((item) => item.id !== id)
      .map((item) => (
        item.parentId === id ? { ...item, parentId: nextParentId } : item
      ))
  }

  async updateOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void> {
    const profile = this.profiles.find((p) => p.id === id)
    if (profile) {
      profile.parentId = newParentId
      profile.order = newOrder
      return
    }
    const folder = this.folders.find((f) => f.id === id)
    if (folder) {
      folder.parentId = newParentId
      folder.order = newOrder
    }
  }

  async listCommandFolders(): Promise<CommandFolder[]> {
    return [...this.commandFolders]
  }

  async createCommandFolder(name: string, parentId?: string): Promise<CommandFolder> {
    const id = globalThis.crypto?.randomUUID?.() ?? `command-folder-${Date.now()}`
    const folder: CommandFolder = { id, type: 'command-folder', name, parentId }
    this.commandFolders.push(folder)
    return folder
  }

  async updateCommandFolder(id: string, updates: Partial<CommandFolder>): Promise<CommandFolder> {
    const folder = this.commandFolders.find((item) => item.id === id)
    if (!folder) throw new Error('Command folder not found')
    Object.assign(folder, updates)
    return folder
  }

  async deleteCommandFolder(id: string): Promise<void> {
    const folder = this.commandFolders.find((item) => item.id === id)
    if (!folder) {
      return
    }
    const nextParentId = folder.parentId
    this.commandFolders = this.commandFolders
      .filter((item) => item.id !== id)
      .map((item) => (
        item.parentId === id ? { ...item, parentId: nextParentId } : item
      ))
    this.commandTemplates = this.commandTemplates.map((item) => (
      item.parentId === id ? { ...item, parentId: nextParentId } : item
    ))
  }

  async listCommandTemplates(): Promise<CommandTemplate[]> {
    return [...this.commandTemplates]
  }

  async createCommandTemplate(input: CommandTemplateInput): Promise<CommandTemplate> {
    const id = globalThis.crypto?.randomUUID?.() ?? `command-${Date.now()}`
    const command = toCommandTemplate(id, input)
    this.commandTemplates = [command, ...this.commandTemplates]
    return command
  }

  async updateCommandTemplate(id: string, input: CommandTemplateInput): Promise<CommandTemplate> {
    const command = toCommandTemplate(id, input)
    this.commandTemplates = this.commandTemplates.map((item) => (item.id === id ? command : item))
    return command
  }

  async getCommandTemplateById(id: string): Promise<CommandTemplate | null> {
    return this.commandTemplates.find((item) => item.id === id) ?? null
  }

  async deleteCommandTemplate(id: string): Promise<void> {
    this.commandTemplates = this.commandTemplates.filter((item) => item.id !== id)
  }

  async updateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<void> {
    const folder = this.commandFolders.find((item) => item.id === id)
    if (folder) {
      folder.parentId = newParentId
      folder.order = newOrder
      return
    }

    const command = this.commandTemplates.find((item) => item.id === id)
    if (command) {
      command.parentId = newParentId
      command.order = newOrder
    }
  }

  async getTerminalCommandHistory(_profileId: string): Promise<TerminalCommandHistoryEntry[]> {
    return []
  }

  async setTerminalCommandHistory(_profileId: string, _entries: TerminalCommandHistoryEntry[]): Promise<void> {
    return
  }

  async getCommandSendPreferences(): Promise<CommandSendPreferences> {
    return {
      rememberSelection: false,
      sendScope: 'current',
      selectedTabIds: []
    }
  }

  async setCommandSendPreferences(_preferences: CommandSendPreferences): Promise<void> {
    return
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

function toProfile(id: string, input: CreateProfileInput): ConnectionProfile {
  return input.type === 'ssh'
    ? {
        id,
        type: 'ssh',
        name: input.name,
        host: input.host,
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

function toCommandTemplate(id: string, input: CommandTemplateInput): CommandTemplate {
  return {
    id,
    type: 'command-template',
    name: input.name,
    command: input.command,
    description: input.description,
    parentId: input.parentId,
    order: input.order,
    appendCarriageReturn: input.appendCarriageReturn ?? true
  }
}
