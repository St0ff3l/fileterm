import { useState, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { t } from '../../i18n'
import { AppIcon } from './AppIcon'
import type { SendScope, SessionSendTarget } from './session-send-targets'

export function SessionSendTargetPicker({
  scope,
  selectedTabIds,
  targets,
  onScopeChange,
  onSelectedTabIdsChange,
  rememberSelection,
  onRememberSelectionChange,
  showRememberSelection = false,
  currentLabel,
  allLabel,
  popover = false
}: {
  scope: SendScope
  selectedTabIds: string[]
  targets: SessionSendTarget[]
  onScopeChange(scope: SendScope): void
  onSelectedTabIdsChange(tabIds: string[]): void
  rememberSelection?: boolean
  onRememberSelectionChange?: (nextValue: boolean) => void
  showRememberSelection?: boolean
  currentLabel?: string
  allLabel?: string
  popover?: boolean
}) {
  const [isOpen, setIsOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const wrapperRef = useRef<HTMLDivElement | null>(null)
  const dropdownRef = useRef<HTMLDivElement | null>(null)
  const [dropdownStyle, setDropdownStyle] = useState<React.CSSProperties>({ display: 'none' })

  const updatePosition = () => {
    if (wrapperRef.current) {
      const rect = wrapperRef.current.getBoundingClientRect()
      const dropdownWidth = 250
      const style: React.CSSProperties = {
        position: 'fixed',
        zIndex: 9999,
        width: `${dropdownWidth}px`,
        right: 'auto'
      }

      style.left = `${rect.right - dropdownWidth}px`

      const spaceBelow = window.innerHeight - rect.bottom
      const spaceAbove = rect.top
      const shouldOpenUpwards = !popover || (spaceBelow < 280 && spaceAbove > spaceBelow)

      if (shouldOpenUpwards) {
        style.bottom = `${window.innerHeight - rect.top + 6}px`
        style.top = 'auto'
      } else {
        style.top = `${rect.bottom + 6}px`
        style.bottom = 'auto'
      }

      setDropdownStyle(style)
    }
  }

  useEffect(() => {
    if (isOpen) {
      updatePosition()
      window.addEventListener('resize', updatePosition)
      window.addEventListener('scroll', updatePosition, true)
    } else {
      setDropdownStyle({ display: 'none' })
    }

    return () => {
      window.removeEventListener('resize', updatePosition)
      window.removeEventListener('scroll', updatePosition, true)
    }
  }, [isOpen, scope, targets, selectedTabIds])

  useEffect(() => {
    if (!isOpen) return

    function handleClickOutside(event: MouseEvent) {
      const target = event.target as Node
      const clickedInsideTrigger = containerRef.current && containerRef.current.contains(target)
      const clickedInsideDropdown = dropdownRef.current && dropdownRef.current.contains(target)

      if (!clickedInsideTrigger && !clickedInsideDropdown) {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => {
      document.removeEventListener('mousedown', handleClickOutside)
    }
  }, [isOpen])

  const handleScopeSelect = (nextScope: SendScope) => {
    onScopeChange(nextScope)
    if (nextScope !== 'selected-ssh') {
      setIsOpen(false)
    }
  }

  const currentLabelText =
    scope === 'current'
      ? (currentLabel ?? t.commandSendCurrent)
      : scope === 'all-ssh'
        ? (allLabel ?? t.commandSendAll)
        : t.commandSendSelected

  return (
    <div className="session-send-target-picker" ref={containerRef}>
      <div className="command-target-select">
        <span>{t.commandSendScope}</span>
        <div className="custom-select-wrapper" ref={wrapperRef}>
          <button type="button" className="custom-select-trigger" onClick={() => setIsOpen((prev) => !prev)}>
            <span>{currentLabelText}</span>
            <AppIcon name="chevron-down" size={10} />
          </button>

          {isOpen &&
            createPortal(
              <div className="custom-select-dropdown" style={dropdownStyle} ref={dropdownRef}>
                <div
                  className={`custom-select-option ${scope === 'current' ? 'is-active' : ''}`}
                  onClick={() => handleScopeSelect('current')}
                >
                  {currentLabel ?? t.commandSendCurrent}
                </div>
                <div
                  className={`custom-select-option ${scope === 'all-ssh' ? 'is-active' : ''}`}
                  onClick={() => handleScopeSelect('all-ssh')}
                >
                  {allLabel ?? t.commandSendAll}
                </div>
                <div
                  className={`custom-select-option ${scope === 'selected-ssh' ? 'is-active' : ''}`}
                  onClick={() => handleScopeSelect('selected-ssh')}
                >
                  {t.commandSendSelected}
                </div>

                {scope === 'selected-ssh' && (
                  <div className="session-send-target-list" onClick={(e) => e.stopPropagation()}>
                    {targets.length ? (
                      <>
                        <div className="session-send-target-header">
                          <label className="session-send-target-header-label">
                            <input
                              type="checkbox"
                              checked={selectedTabIds.length === targets.length}
                              ref={(el) => {
                                if (el) {
                                  el.indeterminate = selectedTabIds.length > 0 && selectedTabIds.length < targets.length
                                }
                              }}
                              onChange={(event) => {
                                if (event.currentTarget.checked) {
                                  onSelectedTabIdsChange(targets.map((t) => t.tabId))
                                } else {
                                  onSelectedTabIdsChange([])
                                }
                              }}
                            />
                            <span className="session-send-target-header-text">
                              {t.commandSendSelected} ({selectedTabIds.length}/{targets.length})
                            </span>
                          </label>
                        </div>

                        <div className="session-send-target-items">
                          {targets.map((target) => {
                            const checked = selectedTabIds.includes(target.tabId)
                            const currentTag = target.isCurrent ? t.commandCurrentBadge : ''

                            return (
                              <label
                                key={target.tabId}
                                className={`session-send-target-item ${checked ? 'is-checked' : ''} ${target.isCurrent ? 'is-current' : ''}`}
                              >
                                <input
                                  checked={checked}
                                  type="checkbox"
                                  onChange={(event) => {
                                    if (event.currentTarget.checked) {
                                      onSelectedTabIdsChange([...selectedTabIds, target.tabId])
                                    } else {
                                      onSelectedTabIdsChange(selectedTabIds.filter((tabId) => tabId !== target.tabId))
                                    }
                                  }}
                                />
                                <span className="session-send-target-icon">
                                  <AppIcon name="brand" size={12} />
                                </span>
                                <span className="session-send-target-index">#{target.index}</span>
                                <span className="session-send-target-title" title={target.title}>
                                  {target.title}
                                </span>
                                {target.isCurrent && (
                                  <span className="session-send-target-current-badge">{currentTag}</span>
                                )}
                              </label>
                            )
                          })}
                        </div>
                      </>
                    ) : (
                      <div className="session-send-target-empty">{t.commandNoAvailableTargets}</div>
                    )}
                  </div>
                )}
              </div>,
              document.body
            )}
        </div>

        {/* Remember Selection on same line for popover mode in select mode */}
        {popover && showRememberSelection && scope === 'selected-ssh' && (
          <label className="session-send-target-remember-inline">
            <input
              checked={rememberSelection ?? false}
              type="checkbox"
              onChange={(event) => onRememberSelectionChange?.(event.currentTarget.checked)}
            />
            <span>{t.commandRememberSelection}</span>
          </label>
        )}
      </div>

      {!popover && showRememberSelection ? (
        <label className="session-send-target-remember">
          <input
            checked={rememberSelection ?? false}
            type="checkbox"
            onChange={(event) => onRememberSelectionChange?.(event.currentTarget.checked)}
          />
          <span>{t.commandRememberSelection}</span>
        </label>
      ) : null}
    </div>
  )
}
