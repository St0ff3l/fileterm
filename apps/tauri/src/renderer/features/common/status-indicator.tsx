import type { HTMLAttributes } from 'react'

export type StatusIndicatorStatus = 'connected' | 'disconnected' | 'connecting' | 'idle'
export type StatusIndicatorSize = 'sm' | 'md'

export function StatusIndicator({
  className,
  size = 'sm',
  status,
  ...spanProps
}: Omit<HTMLAttributes<HTMLSpanElement>, 'children'> & {
  size?: StatusIndicatorSize
  status: StatusIndicatorStatus
}) {
  const resolvedClassName = ['status-indicator', `status-indicator--${size}`, className].filter(Boolean).join(' ')

  return <span {...spanProps} className={resolvedClassName} data-status={status} />
}
