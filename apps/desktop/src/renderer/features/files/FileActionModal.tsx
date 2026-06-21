import { useEffect, useState } from 'react'
import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

export function FileActionModal({
  confirmLabel,
  danger,
  description,
  errorMessage,
  hint,
  initialValue = '',
  inputLabel,
  inputPlaceholder,
  isSubmitting = false,
  onClose,
  onConfirm,
  title
}: {
  confirmLabel: string
  danger?: boolean
  description?: string
  errorMessage?: string | null
  hint?: string
  initialValue?: string
  inputLabel?: string
  inputPlaceholder?: string
  isSubmitting?: boolean
  onClose(): void
  onConfirm(value: string): void
  title: string
}) {
  const [value, setValue] = useState(initialValue)

  useEffect(() => {
    setValue(initialValue)
  }, [initialValue])

  return (
    <div className="modal-backdrop">
      <div className="modal-card file-action-modal">
        <div className="modal-header">
          <span>{title}</span>
          <button className="icon-button" disabled={isSubmitting} onClick={onClose} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>
        {description ? <div className="file-action-description">{description}</div> : null}
        {inputLabel ? (
          <label className="file-action-field">
            <span>{inputLabel}</span>
            <input
              autoFocus
              disabled={isSubmitting}
              placeholder={inputPlaceholder}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !isSubmitting) {
                  onConfirm(value)
                }
              }}
            />
          </label>
        ) : null}
        {hint ? <div className="file-action-hint">{hint}</div> : null}
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onClose} type="button">{t.cancel}</button>
          <button
            className={danger ? 'flat-button danger file-action-submit-button' : 'primary-button file-action-submit-button'}
            disabled={isSubmitting}
            onClick={() => onConfirm(value)}
            type="button"
          >
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{confirmLabel}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
