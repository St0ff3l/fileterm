import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'
import './button.css'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'
export type ButtonSize = 'sm' | 'md' | 'lg' | 'compact' | 'regular' | 'prominent' | 'panel'

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  ref?: Ref<HTMLButtonElement>
  variant?: ButtonVariant
  size?: ButtonSize
  loading?: boolean
  /** Keep the label footprint while showing a centered loading indicator. */
  reserveLoadingSpace?: boolean
  icon?: ReactNode
}

export function Button({
  ref,
  variant = 'secondary',
  size = 'md',
  loading = false,
  reserveLoadingSpace = false,
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
    reserveLoadingSpace ? 'ft-btn--reserve-loading' : '',
    disabled ? 'ft-btn--disabled' : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  const indicator = loading ? (
    <span className="ft-btn__spinner" aria-hidden="true" />
  ) : icon ? (
    <span className="ft-btn__icon">{icon}</span>
  ) : null

  return (
    <button
      ref={ref}
      type={type}
      className={classes}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...rest}
    >
      {reserveLoadingSpace ? (
        <>
          <span className="ft-btn__loading-slot" aria-hidden="true">
            {loading ? indicator : null}
          </span>
          <span className="ft-btn__label">
            {icon ? <span className="ft-btn__icon">{icon}</span> : null}
            {children}
          </span>
        </>
      ) : (
        <>
          {indicator}
          {children}
        </>
      )}
    </button>
  )
}
