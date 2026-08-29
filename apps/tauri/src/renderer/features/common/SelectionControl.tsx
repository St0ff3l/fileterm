import type { ChangeEventHandler } from 'react'

export type SelectionControlType = 'checkbox' | 'radio'

/**
 * Shared native selection input. The visual indicator is supplied by the
 * common control skin, while the browser keeps the correct checkbox/radio
 * semantics and keyboard behavior.
 */
export function SelectionControl({
  checked,
  className,
  disabled = false,
  name,
  onChange,
  type,
  value
}: {
  checked: boolean
  className?: string
  disabled?: boolean
  name?: string
  onChange: ChangeEventHandler<HTMLInputElement>
  type: SelectionControlType
  value?: string
}) {
  return (
    <input
      checked={checked}
      className={`selection-control selection-control--${type} ${className ?? ''}`.trim()}
      disabled={disabled}
      name={name}
      type={type}
      value={value}
      onChange={onChange}
    />
  )
}
