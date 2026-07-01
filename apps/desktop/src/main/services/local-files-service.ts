import { chmod, cp, mkdir, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import type { LocalFileItem, PermissionChangeOptions } from '@fileterm/core'
import { decodeBuffer, encodeText } from './text-encoding.js'

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = -1
  do {
    value /= 1000
    unit += 1
  } while (value >= 1000 && unit < units.length - 1)
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`
}

function formatDate(date: Date) {
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  const hours = `${date.getHours()}`.padStart(2, '0')
  const minutes = `${date.getMinutes()}`.padStart(2, '0')
  return `${year}/${month}/${day} ${hours}:${minutes}`
}

export class LocalFilesService {
  readonly initialPath = os.homedir()

  async listDirectory(dirPath = this.initialPath): Promise<{ path: string, items: LocalFileItem[] }> {
    const entries = await readdir(dirPath, { withFileTypes: true })
    const rows = await Promise.all(entries.map(async (entry) => {
      const fullPath = path.join(dirPath, entry.name)
      const info = await stat(fullPath)
      return {
        path: fullPath,
        name: entry.name,
        type: entry.isDirectory() ? 'folder' : 'file',
        modified: formatDate(info.mtime),
        size: entry.isDirectory() ? '-' : formatSize(info.size),
        permission: formatPermissionBits(info.mode, entry.isDirectory()),
        ownerGroup: typeof info.uid === 'number' && typeof info.gid === 'number' ? `${info.uid}/${info.gid}` : ''
      } satisfies LocalFileItem
    }))

    rows.sort((a, b) => {
      if (a.type !== b.type) {
        return a.type === 'folder' ? -1 : 1
      }
      return a.name.localeCompare(b.name)
    })

    return {
      path: dirPath,
      items: rows
    }
  }

  async readFile(filePath: string, encoding = 'utf-8'): Promise<string> {
    const buffer = await readFile(filePath)
    return decodeBuffer(buffer, encoding)
  }

  async writeFile(filePath: string, content: string, encoding = 'utf-8'): Promise<void> {
    await writeFile(filePath, encodeText(content, encoding))
  }

  async createDirectory(dirPath: string, name: string): Promise<void> {
    await mkdir(path.join(dirPath, name), { recursive: true })
  }

  async createFile(dirPath: string, name: string): Promise<void> {
    const targetPath = path.join(dirPath, name)
    await mkdir(path.dirname(targetPath), { recursive: true })
    await writeFile(targetPath, '', 'utf8')
  }

  async copyPath(sourcePath: string, destinationPath: string): Promise<void> {
    if (sourcePath === destinationPath) {
      return
    }
    await mkdir(path.dirname(destinationPath), { recursive: true })
    await cp(sourcePath, destinationPath, {
      recursive: true,
      errorOnExist: true,
      force: false,
      preserveTimestamps: true
    })
  }

  async movePath(sourcePath: string, destinationPath: string): Promise<void> {
    if (sourcePath === destinationPath) {
      return
    }
    await mkdir(path.dirname(destinationPath), { recursive: true })
    try {
      await rename(sourcePath, destinationPath)
    } catch (error) {
      if (!isCrossDeviceRenameError(error)) {
        throw error
      }
      await this.copyPath(sourcePath, destinationPath)
      await rm(sourcePath, { recursive: true, force: true })
    }
  }

  async renamePath(targetPath: string, newName: string): Promise<void> {
    await rename(targetPath, path.join(path.dirname(targetPath), newName))
  }

  async deletePath(targetPath: string): Promise<void> {
    await rm(targetPath, { recursive: true, force: true })
  }

  async changePermissions(targetPath: string, options: PermissionChangeOptions): Promise<void> {
    const parsedMode = parseMode(options.mode)
    await chmod(targetPath, parsedMode)

    if (!options.recursive) {
      return
    }

    const targetInfo = await stat(targetPath)
    if (!targetInfo.isDirectory()) {
      return
    }

    await this.applyPermissionsRecursively(targetPath, parsedMode, options.applyTo ?? 'all')
  }

  private async applyPermissionsRecursively(targetPath: string, mode: number, applyTo: PermissionChangeOptions['applyTo']) {
    const entries = await readdir(targetPath, { withFileTypes: true })
    for (const entry of entries) {
      const fullPath = path.join(targetPath, entry.name)
      const isDirectory = entry.isDirectory()
      const shouldApply = applyTo === 'all'
        || (applyTo === 'files' && !isDirectory)
        || (applyTo === 'directories' && isDirectory)

      if (shouldApply) {
        await chmod(fullPath, mode)
      }

      if (isDirectory) {
        await this.applyPermissionsRecursively(fullPath, mode, applyTo)
      }
    }
  }
}

function parseMode(mode: string) {
  const normalized = mode.trim()
  if (!/^[0-7]{3,4}$/.test(normalized)) {
    throw new Error('权限值必须是 3 到 4 位八进制数字，例如 755')
  }
  return Number.parseInt(normalized, 8)
}

function isCrossDeviceRenameError(error: unknown) {
  return Boolean(error && typeof error === 'object' && 'code' in error && (error as { code?: string }).code === 'EXDEV')
}

function formatPermissionBits(mode: number, isDirectory: boolean) {
  const segments = [
    [0o400, 0o200, 0o100],
    [0o040, 0o020, 0o010],
    [0o004, 0o002, 0o001]
  ]

  return `${isDirectory ? 'd' : '-'}${segments.map(([read, write, execute]) => {
    return `${mode & read ? 'r' : '-'}${mode & write ? 'w' : '-'}${mode & execute ? 'x' : '-'}`
  }).join('')}`
}
