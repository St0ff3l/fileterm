import { useId, type ReactNode } from 'react'
import { SelectionControl } from './selection-control'

export type RadioCardOption<Value extends string> = {
  value: Value
  label: ReactNode
  description?: ReactNode
  disabled?: boolean
}

/**
 * Reusable card-style radio group.
 *
 * The card is only the presentation layer; the actual control remains a
 * native radio input so browser, keyboard and assistive-technology semantics
 * stay consistent across settings and other desktop surfaces.
 */
export function RadioCardGroup<Value extends string>({
  ariaLabel,
  className,
  disabled = false,
  name,
  onChange,
  options,
  value
}: {
  ariaLabel: string
  className?: string
  disabled?: boolean
  name?: string
  onChange(value: Value): void
  options: readonly RadioCardOption<Value>[]
  value: Value
}) {
  const generatedNameId = useId()

  return (
    <div aria-label={ariaLabel} className={`radio-card-group ${className ?? ''}`.trim()} role="radiogroup">
      {options.map((option) => {
        const selected = value === option.value
        const optionDisabled = disabled || option.disabled

        return (
          <label
            key={option.value}
            className={`radio-card-group__option ${selected ? 'is-selected' : ''} ${optionDisabled ? 'is-disabled' : ''}`.trim()}
          >
            <SelectionControl
              checked={selected}
              disabled={optionDisabled}
              name={name ?? generatedNameId}
              type="radio"
              value={option.value}
              onChange={() => onChange(option.value)}
            />
            <span className="radio-card-group__copy">
              <strong>{option.label}</strong>
              {option.description ? <small>{option.description}</small> : null}
            </span>
          </label>
        )
      })}
    </div>
  )
}
