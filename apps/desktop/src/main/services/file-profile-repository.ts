import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { randomUUID } from 'node:crypto'
import type {
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  CreateProfileInput
} from '@termdock/core'
import type { ProfileRepository } from '@termdock/storage'

export class FileProfileRepository implements ProfileRepository {
  private readonly filePath: string
  private readonly foldersPath: string
  private readonly commandFoldersPath: string
  private readonly commandsPath: string
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
    this.foldersPath = path.join(baseDir, 'folders.json')
    this.commandFoldersPath = path.join(baseDir, 'command-folders.json')
    this.commandsPath = path.join(baseDir, 'commands.json')
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
    const folders = await this.readFolders()
    await this.writeFolders(folders.filter((f) => f.id !== id))
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
    await Promise.all([
      this.writeCommandFolders(folders.filter((item) => item.id !== id)),
      this.writeCommandTemplates(commands.map((item) => (
        item.parentId === id ? { ...item, parentId: undefined } : item
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
    order: input.order ?? Date.now(),
    appendCarriageReturn: input.appendCarriageReturn ?? true
  }
}
