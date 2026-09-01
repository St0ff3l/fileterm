import type { RefObject } from 'react'
import type { AiConversation, AiMessage, AiToolActivity, ActionApprovalRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { AiCopilotMarkdown } from './ai-copilot-markdown'
import { AiCopilotMessageActions } from './ai-copilot-message-actions'
import { AiCopilotToolActivity } from './ai-copilot-tool-activity'

export function AiCopilotMessageList({
  canRetry,
  conversation,
  errorMessage,
  executeCommandInTerminal,
  isStreaming,
  messageViewportRef,
  onDeleteMessage,
  onRetryLastRequest,
  resolveToolApproval,
  resolvingToolApprovalIds,
  toolActivities,
  toolApprovalRequests
}: {
  canRetry: boolean
  conversation: AiConversation | null
  errorMessage: string | null
  executeCommandInTerminal(activity: AiToolActivity): Promise<void>
  isStreaming: boolean
  messageViewportRef: RefObject<HTMLDivElement | null>
  onDeleteMessage(message: AiMessage): void
  onRetryLastRequest(): Promise<void>
  resolveToolApproval(requestId: string, approved: boolean, riskAcknowledged?: boolean): void
  resolvingToolApprovalIds: Set<string>
  toolActivities: AiToolActivity[]
  toolApprovalRequests: ActionApprovalRequest[]
}) {
  return (
    <div className="ai-copilot-message-scroll-region">
      <div
        ref={messageViewportRef}
        className="ai-copilot-message-viewport"
        role="log"
        onMouseDown={(e) => e.stopPropagation()}
        onPointerDown={(e) => e.stopPropagation()}
      >
        {!conversation?.messages.length ? (
          <section className="ai-copilot-empty-chat">
            <span aria-hidden="true" className="material-symbols-outlined">
              forum
            </span>
            <h2>{t.aiCopilotWelcomeTitle}</h2>
            <p>{t.aiCopilotL0Boundary}</p>
          </section>
        ) : (
          conversation.messages.map((message) => (
            <article
              key={message.id}
              className={`ai-copilot-message is-${message.role}`}
              aria-label={message.role === 'user' ? t.aiCopilotMessageUser : t.aiCopilotMessageAssistant}
            >
              <span className="ai-copilot-message-role">
                {message.role === 'user' ? t.aiCopilotMessageUser : t.aiCopilotMessageAssistant}
              </span>
              {message.role === 'assistant' ? (
                <AiCopilotMarkdown content={message.content} />
              ) : (
                <p className="ai-copilot-message-plain">{message.content}</p>
              )}
              {message.toolActivities?.map((activity) => (
                <AiCopilotToolActivity
                  key={activity.proposal.id}
                  activity={activity}
                  approval={
                    activity.proposal.approvalRequestId
                      ? toolApprovalRequests.find(
                          (request) => request.requestId === activity.proposal.approvalRequestId
                        )
                      : undefined
                  }
                  isResolvingApproval={Boolean(
                    activity.proposal.approvalRequestId &&
                    resolvingToolApprovalIds.has(activity.proposal.approvalRequestId)
                  )}
                  onResolveApproval={resolveToolApproval}
                  onExecuteTerminalCommand={executeCommandInTerminal}
                />
              ))}
              <AiCopilotMessageActions
                deleteDisabled={isStreaming}
                text={message.content}
                onDelete={() => {
                  if (!isStreaming) onDeleteMessage(message)
                }}
              />
            </article>
          ))
        )}
        {isStreaming ? (
          <div className="ai-copilot-streaming-indicator">
            <span aria-hidden="true" className="material-symbols-outlined">
              more_horiz
            </span>
            {t.aiCopilotThinking}
          </div>
        ) : null}
        {errorMessage ? (
          <div className="ai-copilot-stream-error" role="alert">
            <span>{errorMessage}</span>
            {canRetry ? (
              <button disabled={isStreaming} type="button" onClick={() => void onRetryLastRequest()}>
                <span aria-hidden="true" className="material-symbols-outlined">
                  refresh
                </span>
                {t.aiCopilotRetry}
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
      <VerticalScrollbar ariaLabel={t.scrollContent} scrollRef={messageViewportRef} />
    </div>
  )
}
