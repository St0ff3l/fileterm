import { useEffect, useState } from 'react'
import type { TransferTask } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { formatTransferBytes, isActiveTransfer, isCompletedTransfer, transferStatusText } from '../../app/app-utils'
import { t } from '../../i18n'

export function TransferPopover({
  onClearTransfers,
  onClose,
  onDiscardTransfer,
  onPauseTransfer,
  onResumeTransfer,
  transfers
}: {
  onClearTransfers(transferIds: string[]): Promise<void> | void
  onClose(): void
  onDiscardTransfer(transferId: string): Promise<void> | void
  onPauseTransfer(transferId: string): Promise<void> | void
  onResumeTransfer(transferId: string): Promise<void> | void
  transfers: TransferTask[]
}) {
  const [statusFilter, setStatusFilter] = useState<'running' | 'completed' | 'all'>('running')
  const [directionFilter, setDirectionFilter] = useState<'all' | 'download' | 'upload'>('all')
  const [pendingActions, setPendingActions] = useState<Record<string, 'pause' | 'resume' | 'discard'>>({})
  const visibleTransfers: TransferTask[] = []
  for (const transfer of transfers) {
    if (statusFilter === 'running' && isCompletedTransfer(transfer)) {
      continue
    }
    if (statusFilter === 'completed' && !isCompletedTransfer(transfer)) {
      continue
    }
    if (directionFilter !== 'all' && transfer.direction !== directionFilter) {
      continue
    }
    visibleTransfers.push(transfer)
    if (visibleTransfers.length === 24) {
      break
    }
  }
  const clearableTransferIds = visibleTransfers
    .filter((transfer) => !isActiveTransfer(transfer) && !transfer.resumable && !transfer.cleanupPending)
    .map((transfer) => transfer.id)

  useEffect(() => {
    setPendingActions((current) =>
      Object.fromEntries(
        Object.entries(current).filter(([id, action]) =>
          transfers.some((transfer) => {
            if (transfer.id !== id) {
              return false
            }
            if (action === 'resume') {
              return transfer.status === 'paused' || transfer.status === 'interrupted'
            }
            return isActiveTransfer(transfer)
          })
        )
      )
    )
  }, [transfers])

  const runAction = (
    transferId: string,
    action: 'pause' | 'resume' | 'discard',
    handler: (id: string) => Promise<void> | void
  ) => {
    setPendingActions((current) => ({ ...current, [transferId]: action }))
    void Promise.resolve()
      .then(() => handler(transferId))
      .catch(() => undefined)
      .finally(() => {
        setPendingActions((current) => {
          const next = { ...current }
          delete next[transferId]
          return next
        })
      })
  }

  const getTransferSizeText = (transfer: TransferTask) => {
    const transferred = formatTransferBytes(transfer.transferredBytes)
    const total = formatTransferBytes(transfer.totalBytes)
    if (transferred && total) {
      return `${transferred} / ${total}`
    }
    return total
  }

  return (
    <section className="transfer-popover">
      <div className="transfer-popover-head">
        <strong>{t.transferDetails}</strong>
        <div className="transfer-popover-actions">
          {statusFilter === 'completed' ? (
            <button
              className="transfer-clear-button"
              disabled={!clearableTransferIds.length}
              onClick={() => {
                if (!clearableTransferIds.length) {
                  return
                }
                void Promise.resolve(onClearTransfers(clearableTransferIds))
              }}
              type="button"
            >
              {t.clearTransferHistory}
            </button>
          ) : null}
          <CloseButton onClick={onClose} />
        </div>
      </div>
      <div className="transfer-filters">
        <div className="transfer-segments">
          <button
            className={statusFilter === 'running' ? 'active' : ''}
            onClick={() => setStatusFilter('running')}
            type="button"
          >
            {t.inProgress}
          </button>
          <button
            className={statusFilter === 'completed' ? 'active' : ''}
            onClick={() => setStatusFilter('completed')}
            type="button"
          >
            {t.completed}
          </button>
          <button
            className={statusFilter === 'all' ? 'active' : ''}
            onClick={() => setStatusFilter('all')}
            type="button"
          >
            {t.all}
          </button>
        </div>
        <div className="transfer-segments transfer-segments-sub">
          <button
            className={directionFilter === 'all' ? 'active' : ''}
            onClick={() => setDirectionFilter('all')}
            type="button"
          >
            {t.all}
          </button>
          <button
            className={directionFilter === 'download' ? 'active' : ''}
            onClick={() => setDirectionFilter('download')}
            type="button"
          >
            {t.download}
          </button>
          <button
            className={directionFilter === 'upload' ? 'active' : ''}
            onClick={() => setDirectionFilter('upload')}
            type="button"
          >
            {t.upload}
          </button>
        </div>
        <small className="transfer-hint">{t.transferUploadHint}</small>
      </div>
      <div className="transfer-popover-list">
        {visibleTransfers.length ? (
          visibleTransfers.map((transfer) => {
            const transferSizeText = getTransferSizeText(transfer)
            const progress = Math.round(Math.max(0, Math.min(100, Number(transfer.progress) || 0)))
            return (
              <div className={`transfer-row transfer-${transfer.status}`} key={transfer.id}>
                <div className="transfer-row-head">
                  <strong title={transfer.name}>{transfer.name}</strong>
                  {(transfer.status === 'running' || transfer.status === 'queued') && transfer.resumable ? (
                    <button
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'pause', onPauseTransfer)}
                      type="button"
                    >
                      {pendingActions[transfer.id] === 'pause' ? t.pausingTransfer : t.pauseTransfer}
                    </button>
                  ) : transfer.status === 'running' || transfer.status === 'queued' ? (
                    <button
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                      type="button"
                    >
                      {pendingActions[transfer.id] === 'discard' ? t.stopping : t.stop}
                    </button>
                  ) : transfer.resumable &&
                    (transfer.status === 'paused' ||
                      transfer.status === 'interrupted' ||
                      transfer.status === 'failed') ? (
                    <span className="transfer-row-actions">
                      <button
                        className="transfer-cancel"
                        disabled={Boolean(pendingActions[transfer.id])}
                        onClick={() => runAction(transfer.id, 'resume', onResumeTransfer)}
                        type="button"
                      >
                        {pendingActions[transfer.id] === 'resume' ? t.resumingTransfer : t.resumeTransfer}
                      </button>
                      <button
                        className="transfer-cancel"
                        disabled={Boolean(pendingActions[transfer.id])}
                        onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                        type="button"
                      >
                        {pendingActions[transfer.id] === 'discard' ? t.discardingCheckpoint : t.discardCheckpoint}
                      </button>
                    </span>
                  ) : transfer.cleanupPending ||
                    transfer.status === 'paused' ||
                    transfer.status === 'interrupted' ||
                    (transfer.status === 'failed' && Boolean(transfer.partialPath)) ? (
                    <button
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                      type="button"
                    >
                      {pendingActions[transfer.id] === 'discard' ? t.discardingCheckpoint : t.discardCheckpoint}
                    </button>
                  ) : null}
                </div>
                <div className="transfer-row-main">
                  <span>{transferStatusText(transfer)}</span>
                </div>
                <div className="transfer-row-meta">
                  <span>
                    {[
                      transfer.direction === 'upload' ? t.upload : t.download,
                      transferSizeText,
                      transfer.speed,
                      `${progress}%`
                    ]
                      .filter(Boolean)
                      .join(' · ')}
                  </span>
                </div>
                <i className="transfer-progress">
                  <b style={{ width: `${progress}%` }} />
                </i>
                {transfer.message ? <small title={transfer.message}>{transfer.message}</small> : null}
              </div>
            )
          })
        ) : (
          <div className="transfer-empty">{t.noTransferTasks}</div>
        )}
      </div>
    </section>
  )
}
