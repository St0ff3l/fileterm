import type { WorkspaceSessionSource } from '@fileterm/core'
import { t } from '../../i18n'
import { StatusIndicator, type StatusIndicatorStatus } from '../common/StatusIndicator'

function sessionSourceLabel(source: WorkspaceSessionSource) {
  return source === 'mcp' ? t.sessionSourceMcp : t.sessionSourceCli
}

export function TransferBar({
  activeCount,
  activeTabId,
  activeTabStatus,
  activeTabSource,
  fullWidth = false,
  isPending,
  onOpen
}: {
  activeCount: number
  activeTabId?: string | null
  activeTabStatus?: StatusIndicatorStatus | null
  activeTabSource?: WorkspaceSessionSource | null
  fullWidth?: boolean
  isPending: boolean
  onOpen(): void
}) {
  return (
    <footer className={`transfer-strip ${fullWidth ? 'full-width' : ''}`}>
      <div className="transfer-strip-context">
        <strong>{t.transferTasks}</strong>
        {activeTabId ? (
          <span className="transfer-session-id" title={activeTabId}>
            <StatusIndicator aria-hidden="true" status={activeTabStatus ?? 'idle'} />
            <span className="transfer-session-id-label">{t.sessionId}</span>
            <code>{activeTabId}</code>
            {activeTabSource ? (
              <span
                className={`transfer-session-source is-${activeTabSource}`}
                title={`${t.sessionSource}: ${sessionSourceLabel(activeTabSource)}`}
              >
                {sessionSourceLabel(activeTabSource)}
              </span>
            ) : null}
          </span>
        ) : null}
      </div>
      <button className="transfer-summary-button" onClick={onOpen} type="button">
        {activeCount > 0 ? `${activeCount} ${t.runningTasks}` : isPending ? t.updating : `0 ${t.runningTasks}`}
      </button>
    </footer>
  )
}
