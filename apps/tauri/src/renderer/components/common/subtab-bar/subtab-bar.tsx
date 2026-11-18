import type { ReactNode } from 'react'
import './subtab-bar.css'

export function SubtabBar<Value extends string>({
  idPrefix,
  ariaLabel,
  className,
  value,
  options,
  onChange
}: {
  idPrefix: string
  ariaLabel: string
  className?: string
  value: Value
  options: readonly { value: Value; label: ReactNode }[]
  onChange(value: Value): void
}) {
  return (
    <div className={className ? `ft-subtab-bar ${className}` : 'ft-subtab-bar'} role="tablist" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option.value}
          id={`${idPrefix}-tab-${option.value}`}
          aria-controls={`${idPrefix}-panel-${option.value}`}
          aria-selected={value === option.value}
          className="ft-subtab-bar__tab"
          role="tab"
          type="button"
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}
