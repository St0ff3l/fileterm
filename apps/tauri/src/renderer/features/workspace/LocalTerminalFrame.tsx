import type { PropsWithChildren } from 'react'

export function LocalTerminalFrame({ children }: PropsWithChildren) {
  return (
    <div className="local-terminal-frame">
      <div className="local-terminal-surface">{children}</div>
    </div>
  )
}
