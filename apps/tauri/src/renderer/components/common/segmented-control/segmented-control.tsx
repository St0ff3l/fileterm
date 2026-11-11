import { useId } from 'react'
import type { ReactNode } from 'react'
import { SelectionControl } from '../../../features/common/selection-control'
import './segmented-control.css'
import { useSelectionIndicator } from '../../../features/common/use-selection-indicator'

export function SegmentedControl<Value extends string>({
  ariaLabel,
  className,
  variant = 'primary',
  value,
  options,
  onChange
}: {
  ariaLabel: string
  className?: string
  variant?: 'primary' | 'neutral'
  value: Value
  options: readonly { value: Value; label: ReactNode }[]
  onChange(value: Value): void
}) {
  const groupId = useId()
  const { rootRef, indicatorRef } = useSelectionIndicator(value, ':scope > .is-selected', options)
  return (
    <div
      ref={rootRef}
      className={`ft-segmented-control${variant === 'neutral' ? ' ft-segmented-control--neutral' : ''}${className ? ` ${className}` : ''}`}
      role="radiogroup"
      aria-label={ariaLabel}
    >
      <span ref={indicatorRef} className="selection-indicator" aria-hidden="true" />
      {options.map((option) => (
        <label
          key={option.value}
          className={`ft-segmented-control__option${value === option.value ? ' is-selected' : ''}`}
        >
          <SelectionControl
            type="radio"
            name={groupId}
            value={option.value}
            checked={value === option.value}
            onChange={() => onChange(option.value)}
          />
          <span className="ft-segmented-control__label">{option.label}</span>
        </label>
      ))}
    </div>
  )
}
