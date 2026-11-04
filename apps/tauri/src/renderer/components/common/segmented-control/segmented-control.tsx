import { useId } from 'react'
import { SelectionControl } from '../../../features/common/selection-control'
import './segmented-control.css'

export function SegmentedControl<Value extends string>({
  ariaLabel,
  value,
  options,
  onChange
}: {
  ariaLabel: string
  value: Value
  options: readonly { value: Value; label: string }[]
  onChange(value: Value): void
}) {
  const groupId = useId()
  return (
    <div className="ft-segmented-control" role="radiogroup" aria-label={ariaLabel}>
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
          <span>{option.label}</span>
        </label>
      ))}
    </div>
  )
}
