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

  add(direction: TransferTask['direction'], name: string) {
    const queuedIndex = this.transfers.findIndex((transfer) => (
      transfer.direction === direction
      && transfer.name === name
      && transfer.status === 'queued'
    ))
    if (queuedIndex !== -1) {
      const queuedTransfer = this.transfers[queuedIndex]
      this.transfers[queuedIndex] = {
        ...queuedTransfer,
        progress: 0,
        status: 'running',
        message: undefined,
        speed: undefined,
        transferredBytes: undefined,
        totalBytes: undefined
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
      totalBytes: undefined
    })
    return transferId
  }

  update(
    transferId: string,
    patch: Partial<Pick<TransferTask, 'progress' | 'status' | 'message' | 'speed' | 'transferredBytes' | 'totalBytes'>>
  ) {
    const index = this.transfers.findIndex((transfer) => transfer.id === transferId)
    if (index === -1) {
      return false
    }

    const current = this.transfers[index]
    if (
      (current.status === 'done' || current.status === 'failed' || current.status === 'canceled')
      && patch.status
      && patch.status !== current.status
    ) {
      return false
    }

    const next = {
      ...current,
      ...patch
    }

    if (
      next.progress === current.progress
      && next.status === current.status
      && next.message === current.message
      && next.speed === current.speed
      && next.transferredBytes === current.transferredBytes
      && next.totalBytes === current.totalBytes
    ) {
      return false
    }

    this.transfers[index] = next
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
