import { randomUUID } from 'node:crypto'
import { rename, stat, unlink } from 'node:fs/promises'
import path from 'node:path'
import type { TransferFileIdentity } from '@fileterm/core'

export const TRANSFER_PART_SUFFIX = '.fileterm-part'

export function localTransferPartialPath(destinationPath: string) {
  return `${destinationPath}${TRANSFER_PART_SUFFIX}`
}

export function remoteTransferPartialPath(destinationPath: string) {
  return `${destinationPath}${TRANSFER_PART_SUFFIX}`
}

export async function statLocalFile(targetPath: string): Promise<TransferFileIdentity | null> {
  try {
    const info = await stat(targetPath)
    if (!info.isFile()) {
      return null
    }
    return {
      size: info.size,
      modifiedAt: info.mtimeMs
    }
  } catch (error) {
    if (isMissingFileError(error)) {
      return null
    }
    throw error
  }
}

export function sameTransferIdentity(current: TransferFileIdentity, expected?: TransferFileIdentity) {
  if (!expected || current.size !== expected.size) {
    return false
  }
  if (expected.modifiedAt === undefined || current.modifiedAt === undefined) {
    return true
  }
  return Math.abs(current.modifiedAt - expected.modifiedAt) < 1
}

export async function replaceLocalFile(partialPath: string, destinationPath: string): Promise<void> {
  const destination = await statLocalFile(destinationPath)
  if (!destination) {
    await rename(partialPath, destinationPath)
    return
  }

  const backupPath = path.join(
    path.dirname(destinationPath),
    `.${path.basename(destinationPath)}.fileterm-backup-${randomUUID()}`
  )

  await rename(destinationPath, backupPath)
  try {
    await rename(partialPath, destinationPath)
  } catch (error) {
    try {
      await rename(backupPath, destinationPath)
    } catch (rollbackError) {
      throw new Error(
        `无法替换下载文件，旧文件保留在 ${backupPath}：${errorMessage(error)}；回滚失败：${errorMessage(rollbackError)}`
      )
    }
    throw error
  }

  await unlink(backupPath).catch(() => undefined)
}

export async function removeLocalFileIfExists(targetPath: string): Promise<void> {
  try {
    await unlink(targetPath)
  } catch (error) {
    if (!isMissingFileError(error)) {
      throw error
    }
  }
}

export function isMissingFileError(error: unknown): boolean {
  return Boolean(
    error && typeof error === 'object' && 'code' in error && (error as NodeJS.ErrnoException).code === 'ENOENT'
  )
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}
