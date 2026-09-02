import type { WorkspaceSessionSource } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StatusIndicator, type StatusIndicatorStatus } from '../common/status-indicator'

function sessionSourceLabel(source: WorkspaceSessionSource) {
  return source === 'mcp' ? t.sessionSourceMcp : t.sessionSourceCli
}

type JumpHostSummary = {
  name: string
  host: string
  port: number
}

export function TransferBar({
  activeCount,
  activeTabId,
  activeTabStatus,
  activeJumpHost,
  activeTabSource,
  fullWidth = false,
  isPending,
  onHideToBackground,
  onOpen
}: {
  activeCount: number
  activeTabId?: string | null
  activeTabStatus?: StatusIndicatorStatus | null
  activeJumpHost?: JumpHostSummary | null
  activeTabSource?: WorkspaceSessionSource | null
  fullWidth?: boolean
  isPending: boolean
  onHideToBackground?(tabId: string): void | Promise<void>
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
            {activeTabSource && onHideToBackground ? (
              <button
                aria-label={`${t.hideSessionToBackground}: ${activeTabId}`}
                className="transfer-session-hide"
                onClick={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  void onHideToBackground(activeTabId)
                }}
                title={t.hideSessionToBackground}
                type="button"
              >
                <AppIcon name="eye-off" size={11} />
              </button>
            ) : null}
          </span>
        ) : null}
        {activeTabId && activeJumpHost ? (
          <span
            aria-label={`${t.viaJumpHost}: ${activeJumpHost.name}`}
            className="transfer-session-jump"
            title={`${t.viaJumpHost}: ${activeJumpHost.name} (${activeJumpHost.host}:${activeJumpHost.port})`}
          >
            <span className="transfer-session-jump-label">{t.viaJumpHost}</span>
            <span className="transfer-session-jump-name">{activeJumpHost.name}</span>
          </span>
        ) : null}
      </div>
      <button className="transfer-summary-button" onClick={onOpen} type="button">
        {activeCount > 0 ? `${activeCount} ${t.runningTasks}` : isPending ? t.updating : `0 ${t.runningTasks}`}
      </button>
    </footer>
  )
}
