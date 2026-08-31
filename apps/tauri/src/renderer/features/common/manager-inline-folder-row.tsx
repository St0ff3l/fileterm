import { useRef, useState, type ReactNode } from 'react'
import { AppIcon } from './app-icon'

export function ManagerInlineFolderRow({
  value,
  placeholder,
  afterNameCells,
  className = '',
  onChange,
  onCommit,
  onDismiss
}: {
  value: string
  placeholder: string
  afterNameCells: ReactNode[]
  className?: string
  onChange(value: string): void
  onCommit(name: string): boolean | void | Promise<boolean | void>
  onDismiss(): void
}) {
  const isCommittingRef = useRef(false)
  const [isCommitting, setIsCommitting] = useState(false)

  const commit = async () => {
    if (isCommittingRef.current) return
    const name = value.trim()
    if (!name) {
      onDismiss()
      return
    }

    isCommittingRef.current = true
    setIsCommitting(true)
    try {
      const result = await onCommit(name)
      if (result !== false) onDismiss()
    } finally {
      isCommittingRef.current = false
      setIsCommitting(false)
    }
  }

  return (
    <div aria-busy={isCommitting} className={`manager-row folder-row ${className}`.trim()}>
      <span className="manager-name-cell">
        <span className="folder-icon manager-folder-toggle">
          <AppIcon name="chevron-right" size={12} />
        </span>
        <input
          autoFocus
          aria-label={placeholder}
          className="manager-inline-input"
          disabled={isCommitting}
          placeholder={placeholder}
          type="text"
          value={value}
          onBlur={() => void commit()}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              void commit()
            } else if (event.key === 'Escape') {
              event.preventDefault()
              onDismiss()
            }
          }}
        />
        <span aria-hidden="true" className="stable-button-icon manager-inline-progress-slot">
          {isCommitting ? <span className="button-spinner" /> : null}
        </span>
      </span>
      {afterNameCells.map((cell, index) => (
        <span key={index}>{cell}</span>
      ))}
    </div>
  )
}
