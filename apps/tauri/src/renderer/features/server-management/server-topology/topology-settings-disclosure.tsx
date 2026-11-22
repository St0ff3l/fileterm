import { useState, type ReactNode, type SyntheticEvent } from 'react'
import { AppIcon } from '../../../features/common/app-icon'

export function TopologySettingsDisclosure({
  storageKey,
  className,
  title,
  children
}: {
  storageKey: string
  className: string
  title: string
  children: ReactNode
}) {
  const [expanded, setExpanded] = useState(() => {
    try {
      return localStorage.getItem(storageKey) === 'true'
    } catch {
      return false
    }
  })

  const rememberExpansion = (event: SyntheticEvent<HTMLDetailsElement>) => {
    const next = event.currentTarget.open
    setExpanded(next)
    try {
      localStorage.setItem(storageKey, String(next))
    } catch {
      // Keep the disclosure usable when persistent storage is unavailable.
    }
  }

  return (
    <details
      className={`server-topology__settings-disclosure ${className}`}
      onToggle={rememberExpansion}
      open={expanded}
    >
      <summary className="server-topology__settings-summary">
        <AppIcon className="server-topology__settings-chevron" name="chevron-right" size={14} />
        <span>{title}</span>
      </summary>
      {children}
    </details>
  )
}
