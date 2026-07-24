import { useMemo, useRef, useState } from 'react'
import type { ConnectionImportConflictStrategy, ConnectionImportPlan } from '@fileterm/core'
import { formatMessage, t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'

export function ConnectionImportPreviewModal({
  plan,
  onClose,
  onCommit
}: {
  plan: ConnectionImportPlan
  onClose(): void
  onCommit(selectedItemIds: string[], conflictStrategy: ConnectionImportConflictStrategy): Promise<void>
}) {
  const readyIds = useMemo(
    () => plan.items.filter((item) => item.status === 'ready' && item.id).map((item) => item.id!),
    [plan]
  )
  const [selected, setSelected] = useState(() => new Set(readyIds))
  const [strategy, setStrategy] = useState<ConnectionImportConflictStrategy>('skip')
  const [isSubmitting, setIsSubmitting] = useState(false)
  const submittingRef = useRef(false)

  const commit = async () => {
    if (submittingRef.current || !selected.size) return
    submittingRef.current = true
    setIsSubmitting(true)
    try {
      await onCommit([...selected], strategy)
    } finally {
      submittingRef.current = false
      setIsSubmitting(false)
    }
  }

  const toggle = (id: string) =>
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  return (
    <div className="modal-backdrop connection-import-preview-backdrop" onClick={isSubmitting ? undefined : onClose}>
      <section className="modal-card connection-import-preview" onClick={(event) => event.stopPropagation()}>
        <header className="connection-manager-header">
          <span className="connection-manager-title">
            <span className="material-symbols-outlined">preview</span>
            <span>{t.connectionImportPreviewTitle}</span>
          </span>
          <CloseButton disabled={isSubmitting} onClick={onClose} />
        </header>
        <p className="connection-import-preview-hint">{t.connectionImportPreviewHint}</p>
        <label className="connection-import-strategy">
          {t.connectionImportConflictLabel}
          <DropdownSelect
            disabled={isSubmitting}
            value={strategy}
            options={[
              { value: 'skip', label: t.connectionImportSkip },
              { value: 'overwrite', label: t.connectionImportOverwrite },
              { value: 'create', label: t.connectionImportCreate }
            ]}
            onChange={(value) => setStrategy(value as ConnectionImportConflictStrategy)}
          />
        </label>
        <div className="connection-import-list">
          {plan.items.map((item) => (
            <label
              key={item.id ?? `${item.sourceLabel}-${item.name}`}
              className={`connection-import-item is-${item.status}`}
            >
              <input
                disabled={isSubmitting || item.status !== 'ready' || !item.id}
                type="checkbox"
                checked={Boolean(item.id && selected.has(item.id))}
                onChange={() => item.id && toggle(item.id)}
              />
              <span className="connection-import-item-main">
                <strong>{item.name}</strong>
                <small>
                  {item.type.toUpperCase()} · {item.host ?? item.sourceLabel ?? '—'}
                  {item.port ? `:${item.port}` : ''}
                  {item.username ? ` · ${item.username}` : ''}
                </small>
                {item.unsupportedFields?.length ? (
                  <small>
                    {t.connectionImportIgnoredFields}
                    {item.unsupportedFields.join(', ')}
                  </small>
                ) : null}
              </span>
              <span className="connection-import-item-status">
                {item.status === 'invalid'
                  ? item.reason
                  : item.conflictProfileId
                    ? t.connectionImportConflictDetected
                    : t.connectionImportReady}
              </span>
            </label>
          ))}
        </div>
        <footer className="connection-import-actions">
          <span>
            {formatMessage(t.connectionImportSelectedCount, { selected: selected.size, total: readyIds.length })}
          </span>
          <button disabled={isSubmitting} type="button" onClick={onClose}>
            {t.cancel}
          </button>
          <button
            className="primary-button compact"
            disabled={!selected.size || isSubmitting}
            type="button"
            onClick={() => void commit()}
          >
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{isSubmitting ? t.connectionImporting : t.connectionImportConfirm}</span>
          </button>
        </footer>
      </section>
    </div>
  )
}
