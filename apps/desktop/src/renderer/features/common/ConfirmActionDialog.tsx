import { createPortal } from 'react-dom'
import type { ReactNode } from 'react'
import { t } from '../../i18n'

export function ConfirmActionDialog({
  cancelLabel = t.cancel,
  confirmLabel,
  confirmDisabled = false,
  confirmVariant = 'danger',
  className,
  description,
  errorMessage,
  extraActions = null,
  isSubmitting = false,
  onClose,
  onConfirm,
  title
}: {
  cancelLabel?: string
  confirmLabel: string
  confirmDisabled?: boolean
  confirmVariant?: 'danger' | 'primary'
  className?: string
  description: ReactNode
  errorMessage?: string | null
  extraActions?: ReactNode
  isSubmitting?: boolean
  onClose(): void
  onConfirm(): void
  title: string
}) {
  const confirmButtonClassName =
    confirmVariant === 'primary'
      ? 'confirm-action-dialog__button confirm-action-dialog__button--primary'
      : 'confirm-action-dialog__button confirm-action-dialog__button--danger'
  const cancelButtonClassName = 'confirm-action-dialog__button confirm-action-dialog__button--secondary'

  const dialog = (
    <div className="modal-backdrop">
      <div
        className={`modal-card confirm-action-dialog${className ? ` ${className}` : ''}`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="confirm-action-dialog__header">
          <div className="confirm-action-dialog__title">{title}</div>
          <div className="confirm-action-dialog__description">{description}</div>
          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        </div>
        <div className="form-actions confirm-action-dialog__footer">
          <button className={cancelButtonClassName} disabled={isSubmitting} onClick={onClose} type="button">
            {cancelLabel}
          </button>
          {extraActions}
          <button
            className={confirmButtonClassName}
            disabled={isSubmitting || confirmDisabled}
            onClick={onConfirm}
            type="button"
          >
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{confirmLabel}</span>
          </button>
        </div>
      </div>
    </div>
  )

  if (typeof document === 'undefined') {
    return dialog
  }

  return createPortal(dialog, document.body)
}
