import type { PropsWithChildren } from 'react'

export function LocalTerminalFrame({ children }: PropsWithChildren) {
  return (
    <div className="local-terminal-frame">
      <div className="local-terminal-frame-left" aria-hidden="true" />
      <div className="local-terminal-surface">{children}</div>
      <div className="local-terminal-frame-right" aria-hidden="true" />
      <div className="local-terminal-frame-bottom" aria-hidden="true" />
    </div>
  )
}
