import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { AiCopilotCopyButton } from './ai-copilot-copy-button'

export function AiCopilotMessageActions({
  text,
  deleteDisabled = false,
  onDelete
}: {
  text: string
  deleteDisabled?: boolean
  onDelete(): void
}) {
  return (
    <div aria-label={t.aiCopilotMessageActions} className="ai-copilot-message-actions">
      <AiCopilotCopyButton
        className="ai-copilot-message-copy-button"
        copiedLabel={t.aiCopilotMessageCopied}
        label={t.aiCopilotCopyMessage}
        text={text}
      />
      <button
        aria-label={t.aiCopilotDeleteMessage}
        className="ai-copilot-message-delete-button"
        disabled={deleteDisabled}
        title={t.aiCopilotDeleteMessage}
        type="button"
        onClick={onDelete}
      >
        <AppIcon name="trash" size={13} />
      </button>
    </div>
  )
}
