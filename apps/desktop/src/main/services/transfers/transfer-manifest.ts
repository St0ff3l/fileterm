import type { TransferFileIdentity, TransferManifest, TransferManifestEntry } from '@fileterm/core'

export type NewTransferManifestEntry = Omit<TransferManifestEntry, 'status' | 'transferredBytes'>

export function createTransferManifest(directories: string[], files: NewTransferManifestEntry[]): TransferManifest {
  return {
    version: 1,
    directories: [...directories],
    files: files.map((file) => ({
      ...file,
      sourceIdentity: { ...file.sourceIdentity },
      status: 'pending',
      transferredBytes: 0
    }))
  }
}

export function updateTransferManifestEntry(
  manifest: TransferManifest,
  relativePath: string,
  patch: Partial<Pick<TransferManifestEntry, 'status' | 'transferredBytes' | 'sourceIdentity'>>
): TransferManifest {
  return {
    ...manifest,
    files: manifest.files.map((file) =>
      file.relativePath === relativePath
        ? {
            ...file,
            ...patch,
            sourceIdentity: patch.sourceIdentity ? { ...patch.sourceIdentity } : file.sourceIdentity
          }
        : file
    )
  }
}

export function transferManifestProgress(manifest: TransferManifest) {
  const totalWeight = manifest.files.reduce((sum, file) => sum + fileWeight(file.sourceIdentity), 0)
  const completedWeight = manifest.files.reduce(
    (sum, file) =>
      sum +
      Math.min(
        fileWeight(file.sourceIdentity),
        file.status === 'done' ? fileWeight(file.sourceIdentity) : Math.max(0, file.transferredBytes)
      ),
    0
  )
  const totalBytes = manifest.files.reduce((sum, file) => sum + file.sourceIdentity.size, 0)
  const transferredBytes = manifest.files.reduce(
    (sum, file) =>
      sum +
      Math.min(
        file.sourceIdentity.size,
        file.status === 'done' ? file.sourceIdentity.size : Math.max(0, file.transferredBytes)
      ),
    0
  )

  return {
    percent: totalWeight === 0 ? 100 : Math.min(99, Math.round((completedWeight / totalWeight) * 100)),
    transferredBytes,
    totalBytes
  }
}

export function isTransferManifestComplete(manifest: TransferManifest) {
  return manifest.files.every((file) => file.status === 'done')
}

export function isValidTransferManifest(value: unknown): value is TransferManifest {
  if (!value || typeof value !== 'object') {
    return false
  }
  const manifest = value as Partial<TransferManifest>
  return (
    manifest.version === 1 &&
    Array.isArray(manifest.directories) &&
    manifest.directories.every((entry) => typeof entry === 'string') &&
    Array.isArray(manifest.files) &&
    manifest.files.every(isValidManifestEntry)
  )
}

function isValidManifestEntry(value: unknown): value is TransferManifestEntry {
  if (!value || typeof value !== 'object') {
    return false
  }
  const entry = value as Partial<TransferManifestEntry>
  return (
    typeof entry.relativePath === 'string' &&
    typeof entry.sourcePath === 'string' &&
    typeof entry.destinationPath === 'string' &&
    typeof entry.partialPath === 'string' &&
    isValidIdentity(entry.sourceIdentity) &&
    (entry.status === 'pending' || entry.status === 'running' || entry.status === 'done') &&
    typeof entry.transferredBytes === 'number' &&
    Number.isFinite(entry.transferredBytes) &&
    entry.transferredBytes >= 0
  )
}

function isValidIdentity(value: unknown): value is TransferFileIdentity {
  if (!value || typeof value !== 'object') {
    return false
  }
  const identity = value as Partial<TransferFileIdentity>
  return (
    typeof identity.size === 'number' &&
    Number.isFinite(identity.size) &&
    identity.size >= 0 &&
    (identity.modifiedAt === undefined ||
      (typeof identity.modifiedAt === 'number' && Number.isFinite(identity.modifiedAt)))
  )
}

function fileWeight(identity: TransferFileIdentity) {
  return Math.max(identity.size, 1)
}
