import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
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
  autoFocus = true,
  className,
  items,
  onClose,
  position,
  viewportMargin = 8
}: {
  align?: 'start' | 'end'
  /** Pointer-opened menus should not draw an initial keyboard focus ring. */
  autoFocus?: boolean
  className?: string
  items: ContextMenuEntry[]
  onClose(): void
  position: { x: number; y: number }
  viewportMargin?: number
}) {
  const menuRef = useRef<HTMLDivElement | null>(null)
  const previousFocusRef = useRef<HTMLElement | null>(null)
  const [resolvedPosition, setResolvedPosition] = useState(position)

  const focusMenuItem = useCallback((direction: 'first' | 'last' | 'next' | 'previous') => {
    const menu = menuRef.current
    if (!menu) return
    const buttons = Array.from(menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'))
    if (!buttons.length) return
    const currentIndex = buttons.indexOf(document.activeElement as HTMLButtonElement)
    const nextIndex =
      direction === 'first'
        ? 0
        : direction === 'last'
          ? buttons.length - 1
          : direction === 'next'
            ? (Math.max(currentIndex, -1) + 1) % buttons.length
            : (currentIndex <= 0 ? buttons.length : currentIndex) - 1
    buttons[nextIndex]?.focus()
  }, [])

  useEffect(() => {
    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
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
    const handleViewportChange = () => onClose()

    window.addEventListener('pointerdown', handlePointerDown, true)
    window.addEventListener('keydown', handleEscape)
    window.addEventListener('blur', handleBlur)
    window.addEventListener('resize', handleViewportChange)
    window.addEventListener('scroll', handleViewportChange, true)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true)
      window.removeEventListener('keydown', handleEscape)
      window.removeEventListener('blur', handleBlur)
      window.removeEventListener('resize', handleViewportChange)
      window.removeEventListener('scroll', handleViewportChange, true)
      const previousFocus = previousFocusRef.current
      if (
        previousFocus?.isConnected &&
        (document.activeElement === document.body || menuRef.current?.contains(document.activeElement))
      ) {
        previousFocus.focus()
      }
    }
  }, [onClose])

  useEffect(() => {
    if (!autoFocus) {
      return
    }
    const frame = window.requestAnimationFrame(() => focusMenuItem('first'))
    return () => window.cancelAnimationFrame(frame)
  }, [autoFocus, focusMenuItem, items, position])

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
      onKeyDown={(event) => {
        if (event.key === 'ArrowDown') {
          event.preventDefault()
          focusMenuItem('next')
        } else if (event.key === 'ArrowUp') {
          event.preventDefault()
          focusMenuItem('previous')
        } else if (event.key === 'Home') {
          event.preventDefault()
          focusMenuItem('first')
        } else if (event.key === 'End') {
          event.preventDefault()
          focusMenuItem('last')
        }
      }}
      role="menu"
      style={{ left: resolvedPosition.x, top: resolvedPosition.y } as CSSProperties}
    >
      {items.map((item, index) =>
        item.separator ? (
          <span key={`sep-${index}`} className="context-menu-separator" role="separator" />
        ) : (
          <button
            key={`${item.label}-${index}`}
            className={item.danger ? 'is-danger' : ''}
            disabled={item.disabled}
            onClick={() => {
              try {
                item.action?.()
              } finally {
                onClose()
              }
            }}
            role="menuitem"
            type="button"
          >
            <span>{item.label}</span>
            {item.shortcut ? <span className="context-menu-shortcut">{item.shortcut}</span> : null}
          </button>
        )
      )}
      <button className="context-close" role="menuitem" type="button" onClick={onClose}>
        {t.closeTab}
      </button>
    </div>
  )

  if (typeof document === 'undefined') {
    return menuElement
  }

  return createPortal(menuElement, document.body)
}
