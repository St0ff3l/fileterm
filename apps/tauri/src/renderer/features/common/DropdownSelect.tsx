import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent
} from 'react'
import { createPortal } from 'react-dom'

export type DropdownOption = {
  value: string
  label: string
  disabled?: boolean
}

// macOS 原生下拉框视觉已经足够好，且与系统语义一致；Windows / Linux 原生
// select 样式与主题脱节，因此仅在这两个平台走自绘 DropdownSelect。
const useNativeSelect = () => window.fileterm?.platform === 'darwin'

export function DropdownSelect({
  value,
  options,
  onChange,
  className,
  placeholder,
  disabled,
  autoFocus,
  menuWidth = 'trigger',
  onKeyDown
}: {
  value: string
  options: DropdownOption[]
  onChange(value: string): void
  className?: string
  placeholder?: string
  disabled?: boolean
  autoFocus?: boolean
  menuWidth?: 'trigger' | 'auto'
  onKeyDown?: (event: ReactKeyboardEvent<HTMLElement>) => void
}) {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const previousFocusRef = useRef<HTMLElement | null>(null)
  const [resolvedStyle, setResolvedStyle] = useState<CSSProperties>({})

  const selectedOption = options.find((option) => option.value === value)
  const selectedLabel = selectedOption?.label ?? placeholder ?? value

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

  const closeMenu = useCallback(() => {
    setOpen(false)
    const previousFocus = previousFocusRef.current
    if (previousFocus?.isConnected) {
      previousFocus.focus()
    }
  }, [])

  const toggleMenu = useCallback(() => {
    if (disabled) return
    if (open) {
      closeMenu()
    } else {
      previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
      setOpen(true)
    }
  }, [disabled, open, closeMenu])

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Node)) return
      if (triggerRef.current?.contains(target)) return
      if (menuRef.current?.contains(target)) return
      closeMenu()
    }
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeMenu()
    }
    window.addEventListener('pointerdown', handlePointerDown, true)
    window.addEventListener('keydown', handleEscape)
    const frame = window.requestAnimationFrame(() => focusMenuItem('first'))
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true)
      window.removeEventListener('keydown', handleEscape)
      window.cancelAnimationFrame(frame)
    }
  }, [open, closeMenu, focusMenuItem])

  useLayoutEffect(() => {
    if (!open) return
    const trigger = triggerRef.current
    const menu = menuRef.current
    if (!trigger || !menu) return

    const rect = trigger.getBoundingClientRect()
    const menuRect = menu.getBoundingClientRect()
    const viewportMargin = 8
    const top = rect.bottom + 4
    const minWidth = menuWidth === 'trigger' ? rect.width : menuRect.width
    const maxLeft = Math.max(viewportMargin, window.innerWidth - minWidth - viewportMargin)
    const left = Math.min(maxLeft, Math.max(viewportMargin, rect.left))
    const maxTop = Math.max(viewportMargin, window.innerHeight - menuRect.height - viewportMargin)

    setResolvedStyle({
      left,
      top: Math.min(maxTop, top),
      minWidth
    })
  }, [open, options, menuWidth])

  const handleSelect = (optionValue: string) => {
    onChange(optionValue)
    closeMenu()
  }

  if (useNativeSelect()) {
    return (
      <span className={`ft-select-shell ${className ?? ''}`.trim()}>
        <select
          autoFocus={autoFocus}
          disabled={disabled}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value} disabled={option.disabled}>
              {option.label}
            </option>
          ))}
        </select>
        <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
          expand_more
        </span>
      </span>
    )
  }

  const menuElement = (
    <div
      ref={menuRef}
      className="context-menu dropdown-select-menu"
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
      style={
        {
          position: 'fixed',
          ...resolvedStyle
        } as CSSProperties
      }
    >
      {options.map((option) => (
        <button
          key={option.value}
          className={option.value === value ? 'is-selected' : ''}
          disabled={option.disabled}
          onClick={() => handleSelect(option.value)}
          role="menuitem"
          type="button"
        >
          <span>{option.label}</span>
          {option.value === value ? (
            <span className="material-symbols-outlined dropdown-select-check">check</span>
          ) : null}
        </button>
      ))}
    </div>
  )

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        autoFocus={autoFocus}
        className={`dropdown-select-trigger ${className ?? ''}`.trim()}
        disabled={disabled}
        onClick={toggleMenu}
        onKeyDown={onKeyDown}
      >
        <span className="dropdown-select-value">{selectedLabel}</span>
        <span className="material-symbols-outlined dropdown-select-arrow">expand_more</span>
      </button>
      {open && typeof document !== 'undefined' ? createPortal(menuElement, document.body) : null}
    </>
  )
}
