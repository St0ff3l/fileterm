import { t } from '../../i18n'
import { CloseButton } from '../common/close-button'

export function AiCopilotHeader({
  canChat,
  isStreaming,
  onClose,
  onOpenSettings,
  openNewConversation
}: {
  canChat: boolean
  isStreaming: boolean
  onClose(): void
  onOpenSettings(): void
  openNewConversation(): void
}) {
  return (
    <header className="ai-copilot-header">
      <div className="ai-copilot-title">
        <span aria-hidden="true" className="material-symbols-outlined ai-copilot-title-icon">
          auto_awesome
        </span>
        <span>
          <strong>{t.aiCopilot}</strong>
        </span>
      </div>
      <div className="ai-copilot-header-actions">
        <button
          aria-label={t.aiCopilotNewChat}
          className="ai-copilot-icon-button"
          disabled={!canChat || isStreaming}
          title={t.aiCopilotNewChat}
          type="button"
          onClick={openNewConversation}
        >
          <span aria-hidden="true" className="material-symbols-outlined">
            add_comment
          </span>
        </button>
        <button
          aria-label={t.aiCopilotConfigureProvider}
          className="ai-copilot-icon-button"
          title={t.aiCopilotConfigureProvider}
          type="button"
          onClick={onOpenSettings}
        >
          <span aria-hidden="true" className="material-symbols-outlined">
            settings_suggest
          </span>
        </button>
        <CloseButton aria-label={t.closeAiCopilot} onClick={onClose} size="compact" />
      </div>
    </header>
  )
}
