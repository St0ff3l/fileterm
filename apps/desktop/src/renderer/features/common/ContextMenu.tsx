import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
import { createPortal } from 'react-dom'
import { t } from '../../i18n'

export type ContextMenuEntry = {
  label?: string
  shortcut?: string
  disabled?: boolean
  danger?: boolean
  action?(): void
  separator?: boolean
}

export function ContextMenu({
  align = 'start',
  className,
  items,
  onClose,
  position,
  viewportMargin = 8
}: {
  align?: 'start' | 'end'
  className?: string
  items: ContextMenuEntry[]
  onClose(): void
  position: { x: number; y: number }
  viewportMargin?: number
}) {
  const menuRef = useRef<HTMLDivElement | null>(null)
  const [resolvedPosition, setResolvedPosition] = useState(position)

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      const menu = menuRef.current
      const target = event.target
      if (!(target instanceof Node) || !menu) {
        onClose()
        return
      }
      if (!menu.contains(target)) {
        onClose()
      }
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    const handleBlur = () => onClose()

    window.addEventListener('pointerdown', handlePointerDown, true)
    window.addEventListener('keydown', handleEscape)
    window.addEventListener('blur', handleBlur)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true)
      window.removeEventListener('keydown', handleEscape)
      window.removeEventListener('blur', handleBlur)
    }
  }, [onClose])

  useLayoutEffect(() => {
    const menu = menuRef.current
    if (!menu) {
      return
    }

    const rect = menu.getBoundingClientRect()
    const left = align === 'end' ? position.x - rect.width : position.x
    const maxLeft = Math.max(viewportMargin, window.innerWidth - rect.width - viewportMargin)
    const maxTop = Math.max(viewportMargin, window.innerHeight - rect.height - viewportMargin)

    setResolvedPosition({
      x: Math.min(maxLeft, Math.max(viewportMargin, left)),
      y: Math.min(maxTop, Math.max(viewportMargin, position.y))
    })
  }, [align, items, position, viewportMargin])

  const menuElement = (
    <div
      ref={menuRef}
      className={`context-menu ${className ?? ''}`.trim()}
      onClick={(event) => event.stopPropagation()}
      style={{ left: resolvedPosition.x, top: resolvedPosition.y } as CSSProperties}
    >
      {items.map((item, index) => item.separator ? (
        <span key={`sep-${index}`} className="context-menu-separator" />
      ) : (
        <button
          key={`${item.label}-${index}`}
          className={item.danger ? 'is-danger' : ''}
          disabled={item.disabled}
          onClick={() => {
            item.action?.()
            onClose()
          }}
          type="button"
        >
          <span>{item.label}</span>
          {item.shortcut ? <span className="context-menu-shortcut">{item.shortcut}</span> : null}
        </button>
      ))}
      <button className="context-close" type="button" onClick={onClose}>{t.closeTab}</button>
    </div>
  )

  if (typeof document === 'undefined') {
    return menuElement
  }

  return createPortal(menuElement, document.body)
}
