import { useEffect, useState } from 'react'
import type { TransferTask } from '@termdock/core'
import { AppIcon } from '../common/AppIcon'
import { isActiveTransfer, isCompletedTransfer, transferStatusText } from '../../app/app-utils'
import { t } from '../../i18n'

export function TransferPopover({
  onCancelTransfer,
  onClearTransfers,
  onClose,
  transfers
}: {
  onCancelTransfer(transferId: string): Promise<void> | void
  onClearTransfers(transferIds: string[]): Promise<void> | void
  onClose(): void
  transfers: TransferTask[]
}) {
  const [statusFilter, setStatusFilter] = useState<'running' | 'completed' | 'all'>('running')
  const [directionFilter, setDirectionFilter] = useState<'all' | 'download' | 'upload'>('all')
  const [pendingCancelIds, setPendingCancelIds] = useState<string[]>([])
  const visibleTransfers = transfers
    .filter((transfer) => {
      if (statusFilter === 'running') {
        return isActiveTransfer(transfer)
      }
      if (statusFilter === 'completed') {
        return isCompletedTransfer(transfer)
      }
      return true
    })
    .filter((transfer) => directionFilter === 'all' || transfer.direction === directionFilter)
    .slice(0, 24)
  const clearableTransferIds = visibleTransfers
    .filter((transfer) => !isActiveTransfer(transfer))
    .map((transfer) => transfer.id)

  useEffect(() => {
    setPendingCancelIds((prev) => prev.filter((id) => transfers.some((transfer) => transfer.id === id && isActiveTransfer(transfer))))
  }, [transfers])

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
          <button className="icon-button" onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>
      </div>
      <div className="transfer-filters">
        <div className="transfer-segments">
          <button className={statusFilter === 'running' ? 'active' : ''} onClick={() => setStatusFilter('running')} type="button">{t.inProgress}</button>
          <button className={statusFilter === 'completed' ? 'active' : ''} onClick={() => setStatusFilter('completed')} type="button">{t.completed}</button>
          <button className={statusFilter === 'all' ? 'active' : ''} onClick={() => setStatusFilter('all')} type="button">{t.all}</button>
        </div>
        <div className="transfer-segments transfer-segments-sub">
          <button className={directionFilter === 'all' ? 'active' : ''} onClick={() => setDirectionFilter('all')} type="button">{t.all}</button>
          <button className={directionFilter === 'download' ? 'active' : ''} onClick={() => setDirectionFilter('download')} type="button">{t.download}</button>
          <button className={directionFilter === 'upload' ? 'active' : ''} onClick={() => setDirectionFilter('upload')} type="button">{t.upload}</button>
        </div>
        <small className="transfer-hint">{t.transferUploadHint}</small>
      </div>
      <div className="transfer-popover-list">
        {visibleTransfers.length ? visibleTransfers.map((transfer) => (
          <div className={`transfer-row transfer-${transfer.status}`} key={transfer.id}>
            <div className="transfer-row-head">
              <strong title={transfer.name}>{transfer.name}</strong>
              {isActiveTransfer(transfer) ? (
                <button
                  className="transfer-cancel"
                  disabled={pendingCancelIds.includes(transfer.id)}
                  onClick={() => {
                    setPendingCancelIds((prev) => prev.includes(transfer.id) ? prev : [...prev, transfer.id])
                    void Promise.resolve(onCancelTransfer(transfer.id)).catch(() => {
                      setPendingCancelIds((prev) => prev.filter((id) => id !== transfer.id))
                    })
                  }}
                  type="button"
                >
                  {pendingCancelIds.includes(transfer.id) ? t.stopping : t.stop}
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
                  transfer.speed,
                  `${transfer.progress}%`
                ].filter(Boolean).join(' · ')}
              </span>
            </div>
            <i className="transfer-progress"><b style={{ width: `${transfer.progress}%` }} /></i>
            {transfer.message ? <small title={transfer.message}>{transfer.message}</small> : null}
          </div>
        )) : (
          <div className="transfer-empty">{t.noTransferTasks}</div>
        )}
      </div>
    </section>
  )
}
