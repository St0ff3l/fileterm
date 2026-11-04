import { useEffect, useState } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'

export function ConnectionSecretField({
  id,
  label,
  value,
  hasSavedValue,
  canClear,
  fullWidth = false,
  optional = false,
  disabled = false,
  annotation,
  annotationTitle,
  onChange,
  onClear,
  onUndo
}: {
  id: string
  label: string
  value: string | null | undefined
  hasSavedValue: boolean
  canClear: boolean
  fullWidth?: boolean
  optional?: boolean
  disabled?: boolean
  annotation?: string
  annotationTitle?: string
  onChange(value: string): void
  onClear(): void
  onUndo(): void
}) {
  const [isVisible, setIsVisible] = useState(false)
  const markedForClear = value === null
  const showClearButton = canClear && hasSavedValue && !markedForClear

  useEffect(() => {
    if (!value || disabled) setIsVisible(false)
  }, [disabled, value])

  return (
    <div className={`ssh-secret-field${fullWidth ? ' span-2' : ''}`}>
      <div className="ssh-secret-field__header">
        <label htmlFor={id}>
          {label}
          {annotation ? (
            <span className="ssh-secret-field__optional" id={`${id}-annotation`} title={annotationTitle}>
              {annotation}
            </span>
          ) : null}
          {optional ? <span className="ssh-secret-field__optional">{t.optionalField}</span> : null}:
        </label>
        {showClearButton ? (
          <button
            className="ssh-secret-field__clear"
            type="button"
            onClick={onClear}
            title={t.clearSavedSecret.replace('{label}', label)}
          >
            <AppIcon name="trash" size={13} />
            {t.clearSavedSecret.replace('{label}', label)}
          </button>
        ) : markedForClear ? (
          <span className="ssh-secret-field__cleared">
            {t.secretMarkedForClear.replace('{label}', label)}
            <button className="ssh-secret-field__undo" type="button" onClick={onUndo}>
              {t.undoClearSavedPassword}
            </button>
          </span>
        ) : null}
      </div>
      <div className="ssh-secret-field__input-wrap">
        <input
          id={id}
          aria-describedby={annotation ? `${id}-annotation` : undefined}
          autoComplete="new-password"
          disabled={disabled || markedForClear}
          placeholder={
            markedForClear
              ? t.secretMarkedForClear.replace('{label}', label)
              : hasSavedValue
                ? t.savedSecretPlaceholder.replace('{label}', label)
                : t.secretPlaceholder.replace('{label}', label)
          }
          type={isVisible ? 'text' : 'password'}
          value={value ?? ''}
          onChange={(event) => onChange(event.target.value)}
        />
        <button
          aria-label={isVisible ? t.hideSecret.replace('{label}', label) : t.showSecret.replace('{label}', label)}
          aria-pressed={isVisible}
          className="ssh-secret-field__toggle"
          disabled={disabled || markedForClear}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setIsVisible((visible) => !visible)}
          title={isVisible ? t.hideSecret.replace('{label}', label) : t.showSecret.replace('{label}', label)}
          type="button"
        >
          <AppIcon name={isVisible ? 'eye-off' : 'eye'} size={15} />
        </button>
      </div>
    </div>
  )
}
