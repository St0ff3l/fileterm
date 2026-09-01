import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'

export function ConnectionSecretField({
  id,
  label,
  value,
  hasSavedValue,
  canClear,
  optional = false,
  disabled = false,
  onChange,
  onClear,
  onUndo
}: {
  id: string
  label: string
  value: string | null | undefined
  hasSavedValue: boolean
  canClear: boolean
  optional?: boolean
  disabled?: boolean
  onChange(value: string): void
  onClear(): void
  onUndo(): void
}) {
  const markedForClear = value === null
  const showClearButton = canClear && hasSavedValue && !markedForClear

  return (
    <div className="ssh-secret-field span-2">
      <div className="ssh-secret-field__header">
        <label htmlFor={id}>
          {label}
          {optional ? <span className="ssh-secret-field__optional">{t.optionalField}</span> : null}:
        </label>
        {showClearButton ? (
          <button className="ssh-secret-field__clear" type="button" onClick={onClear} title={t.clearSavedPassword}>
            <AppIcon name="trash" size={13} />
            {t.clearSavedPassword}
          </button>
        ) : markedForClear ? (
          <span className="ssh-secret-field__cleared">
            {t.passwordMarkedForClear}
            <button className="ssh-secret-field__undo" type="button" onClick={onUndo}>
              {t.undoClearSavedPassword}
            </button>
          </span>
        ) : null}
      </div>
      <input
        id={id}
        autoComplete="new-password"
        disabled={disabled || markedForClear}
        placeholder={
          markedForClear
            ? t.passwordMarkedForClear
            : hasSavedValue
              ? t.passwordReplacePlaceholder
              : t.passwordPlaceholder
        }
        type="password"
        value={value ?? ''}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  )
}
