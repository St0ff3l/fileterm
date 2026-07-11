import { randomUUID } from 'node:crypto'
import type { TransferManifest, TransferProgress, TransferTask } from '@fileterm/core'

export function rootUploadStagingPath() {
  return `/tmp/fileterm-root-upload-${randomUUID()}.part`
}

export function withRootUploadStagingPaths(transfer: TransferTask): TransferTask {
  if (transfer.direction !== 'upload' || transfer.fileAccessMode !== 'root' || !transfer.profileId) {
    return transfer
  }

  const stagingPath = transfer.partialPath ? (transfer.stagingPath ?? rootUploadStagingPath()) : transfer.stagingPath
  let manifest = transfer.manifest
  if (transfer.manifest) {
    let manifestChanged = false
    const files = transfer.manifest.files.map((entry) => {
      if (entry.stagingPath) {
        return entry
      }
      manifestChanged = true
      return {
        ...entry,
        stagingPath: rootUploadStagingPath()
      }
    })
    if (manifestChanged) {
      manifest = { ...transfer.manifest, files }
    }
  }

  if (stagingPath === transfer.stagingPath && manifest === transfer.manifest) {
    return transfer
  }
  return { ...transfer, stagingPath, manifest }
}

export function createTransferSpeedTracker() {
  const minSampleMs = 120
  const smoothingFactor = 0.35
  let sampleStartBytes: number | undefined
  let sampleStartTimestamp: number | undefined
  let smoothedBytesPerSecond: number | undefined
  let lastSpeed: string | undefined

  return (progress: TransferProgress) => {
    if (progress.transferredBytes === undefined) {
      return lastSpeed
    }

    const now = Date.now()
    if (sampleStartBytes === undefined || sampleStartTimestamp === undefined) {
      sampleStartBytes = progress.transferredBytes
      sampleStartTimestamp = now
      return lastSpeed
    }

    const deltaBytes = progress.transferredBytes - sampleStartBytes
    const deltaMs = now - sampleStartTimestamp

    if (deltaBytes <= 0 || deltaMs < minSampleMs) {
      return lastSpeed
    }

    const instantBytesPerSecond = deltaBytes / (deltaMs / 1000)
    smoothedBytesPerSecond =
      smoothedBytesPerSecond === undefined
        ? instantBytesPerSecond
        : smoothedBytesPerSecond * (1 - smoothingFactor) + instantBytesPerSecond * smoothingFactor

    lastSpeed = formatTransferSpeed(smoothedBytesPerSecond)
    sampleStartBytes = progress.transferredBytes
    sampleStartTimestamp = now

    return lastSpeed
  }
}

export function formatTransferByteCount(bytes: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = Math.max(0, bytes)
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  const digits = value >= 100 || unitIndex === 0 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}

export function directoryProgressPercent(
  manifest: TransferManifest,
  relativePath: string,
  currentTransferredBytes: number
) {
  const totalWeight = manifest.files.reduce((sum, entry) => sum + Math.max(entry.sourceIdentity.size, 1), 0)
  if (totalWeight === 0) {
    return 99
  }
  const completedWeight = manifest.files.reduce((sum, entry) => {
    const weight = Math.max(entry.sourceIdentity.size, 1)
    if (entry.status === 'done') {
      return sum + weight
    }
    if (entry.relativePath === relativePath) {
      const currentWeight =
        entry.sourceIdentity.size === 0
          ? currentTransferredBytes > 0
            ? 1
            : 0
          : Math.min(weight, Math.max(0, currentTransferredBytes))
      return sum + currentWeight
    }
    return sum
  }, 0)
  return Math.max(1, Math.min(99, Math.round((completedWeight / totalWeight) * 100)))
}

function formatTransferSpeed(bytesPerSecond: number) {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return undefined
  }

  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  let value = bytesPerSecond
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}
