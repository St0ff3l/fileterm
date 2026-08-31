import type { AiConversation, AiConversationSummary } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'

export function AiCopilotConversationList({
  conversation,
  conversationSearch,
  filteredConversations,
  isStreaming,
  loadConversation,
  onCloseConversationList,
  onDeleteConversation,
  openNewConversation,
  setConversationSearch
}: {
  conversation: AiConversation | null
  conversationSearch: string
  filteredConversations: AiConversationSummary[]
  isStreaming: boolean
  loadConversation(conversationId: string): Promise<void>
  onCloseConversationList(): void
  onDeleteConversation(conversation: AiConversationSummary): void
  openNewConversation(): void
  setConversationSearch(value: string): void
}) {
  return (
    <section className="ai-copilot-conversation-list-page" aria-labelledby="ai-copilot-conversation-list-title">
      <header className="ai-copilot-conversation-list-header">
        <div>
          <span className="ai-copilot-eyebrow">{t.aiCopilotConversationLabel}</span>
          <h2 id="ai-copilot-conversation-list-title">{t.aiCopilotConversationLabel}</h2>
          <p>{t.aiCopilotConversationListDescription}</p>
        </div>
        <button
          className="ai-copilot-conversation-new-button"
          disabled={isStreaming}
          type="button"
          onClick={openNewConversation}
        >
          <span aria-hidden="true" className="material-symbols-outlined">
            add
          </span>
          {t.aiCopilotNewChat}
        </button>
      </header>
      <label className="ai-copilot-conversation-search">
        <span aria-hidden="true" className="material-symbols-outlined">
          search
        </span>
        <span className="ai-copilot-visually-hidden">{t.aiCopilotHistorySearch}</span>
        <input
          aria-label={t.aiCopilotHistorySearch}
          disabled={isStreaming}
          placeholder={t.aiCopilotHistorySearchPlaceholder}
          type="search"
          value={conversationSearch}
          onChange={(event) => setConversationSearch(event.target.value)}
        />
      </label>
      <div className="ai-copilot-conversation-list" role="list">
        {filteredConversations.length ? (
          filteredConversations.map((item) => (
            <div
              key={item.id}
              className={`ai-copilot-conversation-list-item ${item.id === conversation?.id ? 'is-active' : ''}`}
              role="listitem"
            >
              <button
                aria-current={item.id === conversation?.id ? 'true' : undefined}
                className="ai-copilot-conversation-list-item-open"
                disabled={isStreaming}
                type="button"
                onClick={() => {
                  onCloseConversationList()
                  void loadConversation(item.id)
                }}
              >
                <span aria-hidden="true" className="material-symbols-outlined">
                  forum
                </span>
                <span className="ai-copilot-conversation-list-item-copy">
                  <strong>{item.title}</strong>
                  <small>
                    {item.messageCount > 0
                      ? t.aiCopilotConversationMessageCount.replace('{count}', String(item.messageCount))
                      : t.aiCopilotConversationNoMessages}
                  </small>
                </span>
                <span aria-hidden="true" className="material-symbols-outlined">
                  chevron_right
                </span>
              </button>
              <button
                aria-label={`${t.aiCopilotDeleteConversation}: ${item.title}`}
                className="ai-copilot-conversation-list-item-delete"
                disabled={isStreaming && item.id === conversation?.id}
                title={t.aiCopilotDeleteConversation}
                type="button"
                onClick={() => onDeleteConversation(item)}
              >
                <AppIcon name="trash" size={14} />
              </button>
            </div>
          ))
        ) : (
          <div className="ai-copilot-conversation-list-empty">
            <span aria-hidden="true" className="material-symbols-outlined">
              forum
            </span>
            <strong>{t.aiCopilotConversationListEmpty}</strong>
            <p>{t.aiCopilotConversationListDescription}</p>
          </div>
        )}
      </div>
    </section>
  )
}
