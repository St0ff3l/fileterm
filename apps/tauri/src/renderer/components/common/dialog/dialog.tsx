import { useEffect, useRef, type HTMLAttributes, type ReactNode, type Ref } from 'react'
import { CloseButton } from '../../../features/common/close-button'
import './dialog.css'
import { useExitPresence } from '../../../features/common/use-exit-presence'
import { useSurfaceMotion } from '../../../features/common/use-surface-motion'

export type DialogSize = 'sm' | 'md' | 'lg' | 'xl'

export type DialogProps = Omit<HTMLAttributes<HTMLDivElement>, 'title'> & {
  ref?: Ref<HTMLDivElement>
  isOpen: boolean
  onClose: () => void
  title?: ReactNode
  footer?: ReactNode
  size?: DialogSize
  showCloseButton?: boolean
  closeOnBackdropClick?: boolean
  closeOnEscape?: boolean
}

export function Dialog({
  ref,
  isOpen,
  onClose,
  title,
  footer,
  size = 'md',
  showCloseButton = true,
  closeOnBackdropClick = true,
  closeOnEscape = true,
  className = '',
  children,
  ...rest
}: DialogProps) {
  const present = useExitPresence(isOpen)
  const surfaceRef = useRef<HTMLDivElement>(null)
  const backdropRef = useRef<HTMLDivElement>(null)
  useSurfaceMotion(surfaceRef, isOpen, 'scale(0.96)', '--duration-md')
  useSurfaceMotion(backdropRef, isOpen, 'none', '--duration-fade')

  useEffect(() => {
    if (!isOpen || !closeOnEscape) return

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }

    window.addEventListener('keydown', handleKeyDown, { capture: true })
    return () => {
      window.removeEventListener('keydown', handleKeyDown, { capture: true })
    }
  }, [closeOnEscape, isOpen, onClose])

  if (!present) return null

  const dialogClasses = ['ft-dialog', `ft-dialog--${size}`, className].filter(Boolean).join(' ')

  return (
    <div
      ref={backdropRef}
      data-exiting={!isOpen || undefined}
      inert={!isOpen}
      aria-hidden={!isOpen || undefined}
      className="ft-dialog-backdrop"
      onClick={closeOnBackdropClick ? onClose : undefined}
      role="presentation"
    >
      <div
        ref={(node) => {
          surfaceRef.current = node
          if (typeof ref === 'function') return ref(node)
          if (ref) ref.current = node
        }}
        className={dialogClasses}
        role="dialog"
        aria-modal="true"
        onClick={(event) => event.stopPropagation()}
        {...rest}
      >
        {(title || showCloseButton) && (
          <div className="ft-dialog__header">
            {typeof title === 'string' ? <h2 className="ft-dialog__title">{title}</h2> : title || <div />}
            {showCloseButton && <CloseButton onClick={onClose} />}
          </div>
        )}
        <div className="ft-dialog__body">{children}</div>
        {footer && <div className="ft-dialog__footer">{footer}</div>}
      </div>
    </div>
  )
}
