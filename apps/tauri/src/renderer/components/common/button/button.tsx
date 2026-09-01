import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'
import './button.css'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'
export type ButtonSize = 'sm' | 'md' | 'lg' | 'compact' | 'regular' | 'prominent'

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  ref?: Ref<HTMLButtonElement>
  variant?: ButtonVariant
  size?: ButtonSize
  loading?: boolean
  icon?: ReactNode
}

export function Button({
  ref,
  variant = 'secondary',
  size = 'md',
  loading = false,
  icon,
  disabled = false,
  className = '',
  children,
  type = 'button',
  ...rest
}: ButtonProps) {
  const classes = [
    'ft-btn',
    `ft-btn--${variant}`,
    `ft-btn--${size}`,
    loading ? 'ft-btn--loading' : '',
    disabled ? 'ft-btn--disabled' : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <button ref={ref} type={type} className={classes} disabled={disabled || loading} {...rest}>
      {loading ? (
        <span className="ft-btn__spinner" aria-hidden="true" />
      ) : icon ? (
        <span className="ft-btn__icon">{icon}</span>
      ) : null}
      {children}
    </button>
  )
}
