import { randomUUID } from 'node:crypto'
import type { TransferTask } from '@fileterm/core'

export class WorkspaceTransfersState {
  private readonly transfers: TransferTask[]

  constructor(seedTransfers: TransferTask[]) {
    this.transfers = [...seedTransfers]
  }

  list() {
    return [...this.transfers]
  }

  replaceAll(transfers: TransferTask[]) {
    this.transfers.splice(0, this.transfers.length, ...transfers)
  }

  queueUploads(fileNames: string[]) {
    const queued = fileNames.map((name) => ({
      id: randomUUID(),
      direction: 'upload' as const,
      name,
      progress: 0,
      status: 'queued' as const
    }))

    this.transfers.unshift(...queued)
  }

  add(
    direction: TransferTask['direction'],
    name: string,
    details?: Partial<Omit<TransferTask, 'id' | 'direction' | 'name' | 'progress' | 'status'>>
  ) {
    const now = Date.now()
    const queuedIndex = this.transfers.findIndex(
      (transfer) => transfer.direction === direction && transfer.name === name && transfer.status === 'queued'
    )
    if (queuedIndex !== -1) {
      const queuedTransfer = this.transfers[queuedIndex]
      this.transfers[queuedIndex] = {
        ...queuedTransfer,
        progress: 0,
        status: 'running',
        message: undefined,
        speed: undefined,
        transferredBytes: undefined,
        totalBytes: undefined,
        ...details,
        updatedAt: now
      }
      return queuedTransfer.id
    }

    const transferId = randomUUID()
    this.transfers.unshift({
      id: transferId,
      direction,
      name,
      progress: 0,
      status: 'running',
      transferredBytes: undefined,
      totalBytes: undefined,
      ...details,
      createdAt: details?.createdAt ?? now,
      updatedAt: now
    })
    return transferId
  }

  update(transferId: string, patch: Partial<Omit<TransferTask, 'id' | 'direction' | 'name'>>) {
    const index = this.transfers.findIndex((transfer) => transfer.id === transferId)
    if (index === -1) {
      return false
    }

    const current = this.transfers[index]
    if (
      (current.status === 'done' || current.status === 'canceled') &&
      patch.status &&
      patch.status !== current.status
    ) {
      return false
    }

    const hasChanges = Object.entries(patch).some(([key, value]) => current[key as keyof TransferTask] !== value)
    if (!hasChanges) {
      return false
    }

    this.transfers[index] = {
      ...current,
      ...patch,
      updatedAt: Date.now()
    }
    return true
  }

  get(transferId: string) {
    return this.transfers.find((transfer) => transfer.id === transferId)
  }

  removeMany(transferIds: string[]) {
    if (!transferIds.length) {
      return false
    }

    const transferIdSet = new Set(transferIds)
    const nextTransfers = this.transfers.filter((transfer) => !transferIdSet.has(transfer.id))
    if (nextTransfers.length === this.transfers.length) {
      return false
    }

    this.transfers.splice(0, this.transfers.length, ...nextTransfers)
    return true
  }
}
