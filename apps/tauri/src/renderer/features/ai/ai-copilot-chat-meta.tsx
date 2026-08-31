import type { AiConversation } from '@fileterm/core'
import { t } from '../../i18n'

export function AiCopilotChatMeta({
  commandActionMessage,
  conversation,
  conversationTitleDraft,
  deleteCurrentConversation,
  isDeleteConfirmationOpen,
  isRenamingConversation,
  isStreaming,
  onOpenConversationList,
  saveConversationTitle,
  setConversationTitleDraft,
  setIsDeleteConfirmationOpen,
  setIsRenamingConversation
}: {
  commandActionMessage: string | null
  conversation: AiConversation | null
  conversationTitleDraft: string
  deleteCurrentConversation(): Promise<void>
  isDeleteConfirmationOpen: boolean
  isRenamingConversation: boolean
  isStreaming: boolean
  onOpenConversationList(): void
  saveConversationTitle(): Promise<void>
  setConversationTitleDraft(value: string): void
  setIsDeleteConfirmationOpen(value: boolean): void
  setIsRenamingConversation(value: boolean): void
}) {
  return (
    <div className="ai-copilot-chat-meta">
      <div className="ai-copilot-conversation-page-header">
        <div className="ai-copilot-conversation-page-heading">
          <button
            aria-label={t.aiCopilotConversationBack}
            className="ai-copilot-conversation-back"
            disabled={isStreaming}
            title={t.aiCopilotConversationBack}
            type="button"
            onClick={onOpenConversationList}
          >
            <span aria-hidden="true" className="material-symbols-outlined">
              arrow_back
            </span>
          </button>
          <div className="ai-copilot-conversation-page-title">
            <span>{t.aiCopilotConversationLabel}</span>
            <strong title={conversation?.title ?? t.aiCopilotNewChat}>
              {conversation?.title ?? t.aiCopilotNewChat}
            </strong>
          </div>
        </div>
        <section className="ai-copilot-conversation-actions" aria-label={t.aiCopilotConversationControls}>
          {conversation ? (
            isRenamingConversation ? (
              <form
                onSubmit={(event) => {
                  event.preventDefault()
                  void saveConversationTitle()
                }}
              >
                <input
                  aria-label={t.aiCopilotConversationTitle}
                  autoFocus
                  disabled={isStreaming}
                  maxLength={120}
                  value={conversationTitleDraft}
                  onChange={(event) => setConversationTitleDraft(event.target.value)}
                />
                <button disabled={isStreaming || !conversationTitleDraft.trim()} title={t.save} type="submit">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    check
                  </span>
                  {t.save}
                </button>
                <button
                  disabled={isStreaming}
                  title={t.cancel}
                  type="button"
                  onClick={() => {
                    setIsRenamingConversation(false)
                    setConversationTitleDraft(conversation.title)
                  }}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    close
                  </span>
                  {t.cancel}
                </button>
              </form>
            ) : isDeleteConfirmationOpen ? (
              <div className="ai-copilot-conversation-delete-confirm">
                <span>{t.aiCopilotDeleteConversationConfirm}</span>
                <button
                  className="is-danger"
                  disabled={isStreaming}
                  type="button"
                  onClick={() => void deleteCurrentConversation()}
                >
                  {t.delete}
                </button>
                <button disabled={isStreaming} type="button" onClick={() => setIsDeleteConfirmationOpen(false)}>
                  {t.cancel}
                </button>
              </div>
            ) : (
              <div>
                <button
                  aria-label={t.rename}
                  disabled={isStreaming}
                  title={t.rename}
                  type="button"
                  onClick={() => {
                    setConversationTitleDraft(conversation.title)
                    setIsRenamingConversation(true)
                  }}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    edit
                  </span>
                </button>
                <button
                  aria-label={t.aiCopilotDeleteConversation}
                  className="is-danger"
                  disabled={isStreaming}
                  title={t.aiCopilotDeleteConversation}
                  type="button"
                  onClick={() => setIsDeleteConfirmationOpen(true)}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    delete
                  </span>
                </button>
              </div>
            )
          ) : null}
        </section>
      </div>
      {commandActionMessage ? (
        <div className="ai-copilot-command-feedback" role="status" aria-live="polite">
          {commandActionMessage}
        </div>
      ) : null}
    </div>
  )
}
