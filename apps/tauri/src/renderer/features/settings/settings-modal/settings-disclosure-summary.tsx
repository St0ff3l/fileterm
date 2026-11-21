import type { ReactNode } from 'react'
import { AppIcon } from '../../common/app-icon'
import './settings-disclosure-summary.css'

export function SettingsDisclosureSummary({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <summary className={`settings-disclosure-summary ${className}`.trim()}>
      <AppIcon className="settings-disclosure-chevron" name="chevron-right" size={14} />
      {children}
    </summary>
  )
}
