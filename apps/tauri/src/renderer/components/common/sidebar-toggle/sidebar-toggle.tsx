import { Button } from '../button/button'
import './sidebar-toggle.css'

export function SidebarToggle({
  collapsed,
  label,
  className,
  side = 'left',
  onClick
}: {
  collapsed: boolean
  label: string
  /** Context class so callers and tests can distinguish multiple sidebar toggles. */
  className?: string
  /** Right-hand sidebars mirror the glyph so it points at the panel it folds. */
  side?: 'left' | 'right'
  onClick(): void
}) {
  return (
    <Button
      aria-label={label}
      aria-expanded={!collapsed}
      className={`sidebar-toggle ${side === 'right' ? 'is-right' : ''} ${className ?? ''}`.trim()}
      title={label}
      onClick={onClick}
      icon={<span className="sidebar-toggle-icon" aria-hidden="true" />}
    />
  )
}
