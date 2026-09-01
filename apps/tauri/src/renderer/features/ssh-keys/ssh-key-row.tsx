import type { DragEvent, PointerEvent as ReactPointerEvent } from 'react'
import type { SshKeyMetadata } from '@fileterm/core'
import { AppIcon } from '../common/app-icon'
import { t } from '../../i18n'
import { shortFingerprint } from './ssh-key-manager-utils'

export function SshKeyRow({
  item,
  className,
  draggable = false,
  onDelete,
  onEdit,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
  onPointerDown
}: {
  item: SshKeyMetadata
  className?: string
  draggable?: boolean
  onDelete(): void
  onEdit(): void
  onDragStart?(event: DragEvent): void
  onDragOver?(event: DragEvent): void
  onDragLeave?(event: DragEvent): void
  onDrop?(event: DragEvent): void
  onDragEnd?(): void
  onPointerDown?(event: ReactPointerEvent): void
}) {
  return (
    <div
      className={`manager-row ssh-key-manager-row${className ? ` ${className}` : ''}`}
      draggable={draggable}
      data-fileterm-sort-id={item.id}
      data-fileterm-sort-kind="key"
      onPointerDown={onPointerDown}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
    >
      <span className="ssh-key-name-cell">
        <span className="manager-name-cell ssh-key-name-primary">
          <span className="manager-node-icon">
            <AppIcon name="key" size={14} />
          </span>
          <span className="manager-node-name">{item.name}</span>
        </span>
        <small>{item.encrypted ? t.encrypted : t.unencrypted}</small>
      </span>
      <span className="ssh-key-fingerprint-cell">
        <span>{item.algorithm}</span>
        <code title={item.fingerprint}>{shortFingerprint(item.fingerprint)}</code>
      </span>
      <span className="ssh-key-note-cell">{item.note || '—'}</span>
      <span className="ssh-key-imported-at">
        {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(item.importedAt)}
      </span>
      <span>{item.usageCount}</span>
      <span className="manager-actions ssh-key-actions">
        <button
          aria-label={t.editKeyNote}
          className="manager-icon-action"
          title={t.editKeyNote}
          type="button"
          onMouseDown={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            onEdit()
          }}
        >
          <AppIcon name="edit" size={14} />
        </button>
        <button
          aria-label={t.deleteKey}
          className="manager-icon-action danger"
          disabled={item.usageCount > 0}
          title={item.usageCount > 0 ? t.keyStillReferenced : t.deleteKey}
          type="button"
          onMouseDown={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            onDelete()
          }}
        >
          <AppIcon name="trash" size={14} />
        </button>
      </span>
    </div>
  )
}
