import { useRef, useState } from 'react'
import type { AiCommandRisk, AiToolActivity, ActionApprovalRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StableButtonContent } from '../common/stable-button-content'
import { SelectionControl } from '../common/selection-control'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { AiCopilotCopyButton } from './ai-copilot-copy-button'

function commandRiskLabel(risk: AiCommandRisk) {
  switch (risk) {
    case 'read-only':
      return t.aiCopilotRiskReadOnly
    case 'mutating':
      return t.aiCopilotRiskMutating
    case 'destructive':
      return t.aiCopilotRiskDestructive
    case 'privileged':
      return t.aiCopilotRiskPrivileged
    default:
      return t.aiCopilotRiskUnknown
  }
}

export function AiCopilotToolActivity({
  activity,
  approval,
  isResolvingApproval = false,
  onResolveApproval,
  onExecuteTerminalCommand
}: {
  activity: AiToolActivity
  approval?: ActionApprovalRequest
  isResolvingApproval?: boolean
  onResolveApproval?: (requestId: string, approved: boolean, riskAcknowledged?: boolean) => void
  onExecuteTerminalCommand?: (activity: AiToolActivity) => Promise<void>
}) {
  const outputScrollRef = useRef<HTMLPreElement>(null)
  const [riskAcknowledged, setRiskAcknowledged] = useState(false)
  const [isExecutingTerminalCommand, setIsExecutingTerminalCommand] = useState(false)
  const result = activity.result
  const status = result?.status ?? 'pending'
  // Only a semi-automatic proposal has a live approval gate that can be
  // delegated to the visible terminal. Fully automatic calls are already
  // running through the isolated route; exposing this button while their
  // result is pending would execute the same command a second time.
  const canDelegateToTerminal = Boolean(approval && activity.proposal.approvalRequestId && onExecuteTerminalCommand)
  const canExecuteTerminalCommand = Boolean(
    canDelegateToTerminal && !/[\r\n]/.test(activity.proposal.command) && !isExecutingTerminalCommand
  )
  const executeTerminalCommandButton =
    canDelegateToTerminal && onExecuteTerminalCommand && !result ? (
      <button
        aria-label={t.aiCopilotWriteTerminalInput}
        aria-busy={isExecutingTerminalCommand}
        className="ai-copilot-tool-write-button"
        disabled={!canExecuteTerminalCommand || isResolvingApproval}
        title={canExecuteTerminalCommand ? t.aiCopilotWriteTerminalInputHint : t.aiCopilotMultilinePasteUnavailable}
        type="button"
        onClick={() => {
          if (!onExecuteTerminalCommand || !canExecuteTerminalCommand) return
          setIsExecutingTerminalCommand(true)
          void onExecuteTerminalCommand(activity)
            .catch(() => undefined)
            .finally(() => setIsExecutingTerminalCommand(false))
        }}
      >
        <StableButtonContent
          busy={isExecutingTerminalCommand}
          icon={<AppIcon name="terminal-file" size={13} />}
          label={t.aiCopilotWriteTerminalInput}
        />
      </button>
    ) : null
  const statusLabel = result
    ? status === 'executed'
      ? t.aiCopilotToolExecuted
      : status === 'input-required'
        ? t.aiCopilotToolWaitingForInput
        : status === 'executed-in-terminal'
          ? t.aiCopilotToolExecutedInTerminal
          : status === 'cancelled'
            ? t.aiCopilotToolCancelled
            : status === 'rejected' || status === 'auto-blocked'
              ? t.aiCopilotToolRejected
              : t.aiCopilotToolFailed
    : approval
      ? t.aiCopilotToolApprovalPending
      : t.aiCopilotToolPending
  return (
    <section className={`ai-copilot-command-card ai-copilot-tool-activity is-${status}`}>
      <header>
        <span>{statusLabel}</span>
        <span className={`ai-copilot-command-risk is-${activity.proposal.risk}`}>
          {commandRiskLabel(activity.proposal.risk)}
        </span>
      </header>
      <div className="ai-copilot-code-block ai-copilot-command-code-block">
        <code>{activity.proposal.command}</code>
        <AiCopilotCopyButton text={activity.proposal.command} />
      </div>
      {activity.proposal.explanation ? <p>{activity.proposal.explanation}</p> : null}
      {result?.reason && status !== 'executed-in-terminal' ? (
        <p className="is-warning">{result.reason}</p>
      ) : status === 'executed-in-terminal' ? (
        <p>{t.aiCopilotToolExecutedInTerminalDescription}</p>
      ) : null}
      {result?.stdout ? (
        <div className="ai-copilot-tool-output-wrap">
          <pre ref={outputScrollRef} className="ai-copilot-tool-output">
            {result.stdout}
          </pre>
          <VerticalScrollbar ariaLabel={t.aiCopilotToolActivity} scrollRef={outputScrollRef} />
        </div>
      ) : null}
      {approval && !result ? (
        <div className="ai-copilot-tool-approval">
          {approval.target ? <small>{`${t.aiCopilotToolApprovalTarget}：${approval.target}`}</small> : null}
          {approval.requiresRiskAcknowledgement ? (
            <label className="ai-copilot-tool-risk-ack">
              <SelectionControl
                checked={riskAcknowledged}
                disabled={isResolvingApproval || isExecutingTerminalCommand}
                type="checkbox"
                onChange={(event) => setRiskAcknowledged(event.currentTarget.checked)}
              />
              <span>{t.aiCopilotToolApprovalRisk}</span>
            </label>
          ) : null}
          <footer className="ai-copilot-tool-approval-actions">
            <button
              aria-busy={isResolvingApproval}
              disabled={isResolvingApproval || isExecutingTerminalCommand}
              type="button"
              onClick={() => onResolveApproval?.(approval.requestId, false)}
            >
              <StableButtonContent
                busy={isResolvingApproval}
                icon={<AppIcon name="close" size={13} />}
                label={t.aiCopilotToolReject}
              />
            </button>
            {executeTerminalCommandButton}
            <button
              aria-busy={isResolvingApproval}
              disabled={
                isResolvingApproval ||
                isExecutingTerminalCommand ||
                (approval.requiresRiskAcknowledgement && !riskAcknowledged) ||
                !onResolveApproval
              }
              type="button"
              onClick={() => onResolveApproval?.(approval.requestId, true, riskAcknowledged)}
            >
              <StableButtonContent
                busy={isResolvingApproval}
                icon={<AppIcon name="check" size={13} />}
                label={t.aiCopilotToolApprove}
              />
            </button>
          </footer>
        </div>
      ) : executeTerminalCommandButton ? (
        <footer className="ai-copilot-tool-result-actions">{executeTerminalCommandButton}</footer>
      ) : null}
    </section>
  )
}
