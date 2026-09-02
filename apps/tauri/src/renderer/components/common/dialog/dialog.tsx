import { useEffect, type HTMLAttributes, type ReactNode, type Ref } from 'react'
import { CloseButton } from '../../../features/common/close-button'
import './dialog.css'

export type DialogSize = 'sm' | 'md' | 'lg' | 'xl'

export type DialogProps = HTMLAttributes<HTMLDivElement> & {
  ref?: Ref<HTMLDivElement>
  isOpen: boolean
  onClose: () => void
  title?: ReactNode
  footer?: ReactNode
  size?: DialogSize
  showCloseButton?: boolean
  closeOnBackdropClick?: boolean
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
  className = '',
  children,
  ...rest
}: DialogProps) {
  useEffect(() => {
    if (!isOpen) return

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
  }, [isOpen, onClose])

  if (!isOpen) return null

  const dialogClasses = ['ft-dialog', `ft-dialog--${size}`, className].filter(Boolean).join(' ')

  return (
    <div className="ft-dialog-backdrop" onClick={closeOnBackdropClick ? onClose : undefined} role="presentation">
      <div
        ref={ref}
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
