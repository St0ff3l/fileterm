import type { InputHTMLAttributes, Ref } from 'react'

export type SelectionControlType = 'checkbox' | 'radio'
export type SelectionControlSize = 'default' | 'large'

export type SelectionControlProps = Omit<InputHTMLAttributes<HTMLInputElement>, 'className' | 'size' | 'type'> & {
  className?: string
  ref?: Ref<HTMLInputElement>
  size?: SelectionControlSize
  type: SelectionControlType
}

/**
 * Shared native selection input. The visual indicator is supplied by the
 * common control skin, while the browser keeps the correct checkbox/radio
 * semantics and keyboard behavior.
 */
export function SelectionControl({ className, ref, size = 'default', type, ...inputProps }: SelectionControlProps) {
  return (
    <input
      {...inputProps}
      ref={ref}
      className={`selection-control selection-control--${type} selection-control--${size} ${className ?? ''}`.trim()}
      type={type}
    />
  )
}
