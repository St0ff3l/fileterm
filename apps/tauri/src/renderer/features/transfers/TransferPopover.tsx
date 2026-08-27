import { useEffect, useRef, useState } from 'react'
import type { TransferTask } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { StableButtonContent, StableButtonLabel } from '../common/StableButtonContent'
import {
  formatTransferBytes,
  formatTransferDateTime,
  getTransferTimestamp,
  isActiveTransfer,
  isCompletedTransfer,
  transferStatusText
} from '../../app/app-utils'
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
  const [isClearing, setIsClearing] = useState(false)
  const pendingTransferIdsRef = useRef(new Set<string>())
  const isClearingRef = useRef(false)
  const orderedTransfers = transfers
    .map((transfer, index) => ({ index, transfer }))
    .sort((left, right) => {
      const leftTimestamp = getTransferTimestamp(left.transfer) ?? 0
      const rightTimestamp = getTransferTimestamp(right.transfer) ?? 0
      return rightTimestamp - leftTimestamp || left.index - right.index
    })
    .map(({ transfer }) => transfer)
  const visibleTransfers: TransferTask[] = []
  for (const transfer of orderedTransfers) {
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
    if (pendingTransferIdsRef.current.has(transferId)) return
    pendingTransferIdsRef.current.add(transferId)
    setPendingActions((current) => ({ ...current, [transferId]: action }))
    void Promise.resolve()
      .then(() => handler(transferId))
      .catch(() => undefined)
      .finally(() => {
        pendingTransferIdsRef.current.delete(transferId)
        setPendingActions((current) => {
          const next = { ...current }
          delete next[transferId]
          return next
        })
      })
  }

  const clearTransfers = () => {
    if (!clearableTransferIds.length || isClearingRef.current) return
    isClearingRef.current = true
    setIsClearing(true)
    void Promise.resolve()
      .then(() => onClearTransfers(clearableTransferIds))
      .catch(() => undefined)
      .finally(() => {
        isClearingRef.current = false
        setIsClearing(false)
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
              aria-busy={isClearing}
              className="transfer-clear-button"
              disabled={!clearableTransferIds.length || isClearing}
              onClick={clearTransfers}
              type="button"
            >
              <StableButtonContent
                busy={isClearing}
                busyLabel={t.clearingTransferHistory}
                label={t.clearTransferHistory}
              />
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
            const transferTimestamp = getTransferTimestamp(transfer)
            const transferDateTime = formatTransferDateTime(transferTimestamp)
            const temporaryPath =
              transfer.partialPath && transfer.status !== 'done' && transfer.status !== 'canceled'
                ? transfer.partialPath
                : undefined
            const transferMessage =
              transfer.message && transfer.message !== transfer.partialPath ? transfer.message : undefined
            return (
              <div
                aria-busy={Boolean(pendingActions[transfer.id])}
                className={`transfer-row transfer-${transfer.status}`}
                key={transfer.id}
              >
                <div className="transfer-row-head">
                  <strong title={transfer.name}>{transfer.name}</strong>
                  {(transfer.status === 'running' || transfer.status === 'queued') && transfer.resumable ? (
                    <button
                      aria-busy={pendingActions[transfer.id] === 'pause'}
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'pause', onPauseTransfer)}
                      type="button"
                    >
                      <StableButtonLabel
                        busy={pendingActions[transfer.id] === 'pause'}
                        busyLabel={t.pausingTransfer}
                        label={t.pauseTransfer}
                      />
                    </button>
                  ) : transfer.status === 'running' || transfer.status === 'queued' ? (
                    <button
                      aria-busy={pendingActions[transfer.id] === 'discard'}
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                      type="button"
                    >
                      <StableButtonLabel
                        busy={pendingActions[transfer.id] === 'discard'}
                        busyLabel={t.stopping}
                        label={t.stop}
                      />
                    </button>
                  ) : transfer.resumable &&
                    (transfer.status === 'paused' ||
                      transfer.status === 'interrupted' ||
                      transfer.status === 'failed') ? (
                    <span className="transfer-row-actions">
                      <button
                        aria-busy={pendingActions[transfer.id] === 'resume'}
                        className="transfer-cancel"
                        disabled={Boolean(pendingActions[transfer.id])}
                        onClick={() => runAction(transfer.id, 'resume', onResumeTransfer)}
                        type="button"
                      >
                        <StableButtonLabel
                          busy={pendingActions[transfer.id] === 'resume'}
                          busyLabel={t.resumingTransfer}
                          label={t.resumeTransfer}
                        />
                      </button>
                      <button
                        aria-busy={pendingActions[transfer.id] === 'discard'}
                        className="transfer-cancel"
                        disabled={Boolean(pendingActions[transfer.id])}
                        onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                        type="button"
                      >
                        <StableButtonLabel
                          busy={pendingActions[transfer.id] === 'discard'}
                          busyLabel={t.discardingCheckpoint}
                          label={t.discardCheckpoint}
                        />
                      </button>
                    </span>
                  ) : transfer.cleanupPending ||
                    transfer.status === 'paused' ||
                    transfer.status === 'interrupted' ||
                    (transfer.status === 'failed' && Boolean(transfer.partialPath)) ? (
                    <button
                      aria-busy={pendingActions[transfer.id] === 'discard'}
                      className="transfer-cancel"
                      disabled={Boolean(pendingActions[transfer.id])}
                      onClick={() => runAction(transfer.id, 'discard', onDiscardTransfer)}
                      type="button"
                    >
                      <StableButtonLabel
                        busy={pendingActions[transfer.id] === 'discard'}
                        busyLabel={t.discardingCheckpoint}
                        label={t.discardCheckpoint}
                      />
                    </button>
                  ) : null}
                </div>
                <div className="transfer-row-main">
                  <span>{transferStatusText(transfer)}</span>
                  {transferDateTime && transferTimestamp !== undefined ? (
                    <time dateTime={new Date(transferTimestamp).toISOString()}>{transferDateTime}</time>
                  ) : null}
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
                {transfer.destinationPath ? (
                  <small className="transfer-row-path" title={transfer.destinationPath}>
                    {t.transferDestination} {transfer.destinationPath}
                  </small>
                ) : null}
                {temporaryPath ? (
                  <small className="transfer-row-path" title={temporaryPath}>
                    {t.transferTemporaryFile} {temporaryPath}
                  </small>
                ) : null}
                {transferMessage ? (
                  <small className="transfer-row-message" title={transferMessage}>
                    {transfer.manifest && !isCompletedTransfer(transfer) ? (
                      <span className="transfer-row-current-item-label">
                        {transfer.direction === 'download' ? t.currentDownloadItem : t.currentUploadItem}
                      </span>
                    ) : null}
                    {transferMessage}
                  </small>
                ) : null}
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
