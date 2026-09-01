import type { HTMLAttributes, ReactNode, Ref } from 'react'
import './card.css'

export type CardProps = HTMLAttributes<HTMLDivElement> & {
  ref?: Ref<HTMLDivElement>
  hoverable?: boolean
  elevated?: boolean
  flat?: boolean
  header?: ReactNode
  footer?: ReactNode
}

export function Card({
  ref,
  hoverable = false,
  elevated = false,
  flat = false,
  header,
  footer,
  className = '',
  children,
  ...rest
}: CardProps) {
  const classes = [
    'ft-card',
    hoverable ? 'ft-card--hoverable' : '',
    elevated ? 'ft-card--elevated' : '',
    flat ? 'ft-card--flat' : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div ref={ref} className={classes} {...rest}>
      {header && <div className="ft-card__header">{header}</div>}
      <div className="ft-card__body">{children}</div>
      {footer && <div className="ft-card__footer">{footer}</div>}
    </div>
  )
}
