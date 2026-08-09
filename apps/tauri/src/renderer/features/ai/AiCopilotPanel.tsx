import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import type { ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'
import { useAiCopilot } from './useAiCopilot'

export function AiCopilotPanel({
  activeProfile,
  activeSession,
  activeTab,
  onClose,
  onOpenSettings
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  onClose(): void
  onOpenSettings(): void
}) {
  const isSshTarget = activeTab?.sessionType === 'ssh' && Boolean(activeSession)
  const host = activeSession?.accessHost || activeProfile?.host || activeTab?.title || t.aiCopilotNoTerminalTitle
  const user = activeSession?.shellUser || activeSession?.loginUser || activeProfile?.username
  const targetLabel = isSshTarget && user ? `${user}@${host}` : host
  const workingDirectory = activeSession?.shellCwd || activeSession?.remotePath || '~'
  const [draft, setDraft] = useState('')
  const messageViewportRef = useRef<HTMLDivElement>(null)
  const {
    providers,
    conversations,
    conversation,
    currentProvider,
    selectedProviderId,
    isLoading,
    isStreaming,
    errorMessage,
    usage,
    selectProvider,
    loadConversation,
    newChat,
    sendMessage,
    retry,
    stop
  } = useAiCopilot()

  useEffect(() => {
    const viewport = messageViewportRef.current
    if (viewport) {
      viewport.scrollTop = viewport.scrollHeight
    }
  }, [conversation?.messages, isStreaming])

  const canChat = Boolean(currentProvider)
  const canRetry = Boolean(errorMessage && conversation?.messages.at(-1)?.role === 'user')

  const send = async () => {
    const sent = await sendMessage(draft)
    if (sent) {
      setDraft('')
    }
  }

  const onComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== 'Enter' || event.shiftKey || (!event.metaKey && !event.ctrlKey)) return
    event.preventDefault()
    void send()
  }

  return (
    <aside aria-label={t.aiCopilot} className="ai-copilot-panel">
      <header className="ai-copilot-header">
        <div className="ai-copilot-title">
          <span aria-hidden="true" className="material-symbols-outlined ai-copilot-title-icon">
            auto_awesome
          </span>
          <span>
            <strong>{t.aiCopilot}</strong>
            <small>{currentProvider ? currentProvider.name + ' · ' + currentProvider.model : t.aiCopilotPreview}</small>
          </span>
        </div>
        <div className="ai-copilot-header-actions">
          <button
            aria-label={t.aiCopilotNewChat}
            className="ai-copilot-icon-button"
            disabled={!canChat || isStreaming}
            title={t.aiCopilotNewChat}
            type="button"
            onClick={newChat}
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

      <section aria-label={t.aiCopilotCurrentTerminal} className="ai-copilot-context-strip">
        <span aria-hidden="true" className={`ai-copilot-target-icon ${isSshTarget ? 'is-connected' : ''}`}>
          <span className="material-symbols-outlined">terminal</span>
        </span>
        <span className="ai-copilot-target-copy">
          <strong>{targetLabel}</strong>
          <small>{isSshTarget ? workingDirectory : t.aiCopilotNoTerminalDescription}</small>
        </span>
        <span className="ai-copilot-context-status">{t.aiCopilotContextOff}</span>
      </section>

      <div className={`ai-copilot-content ${canChat ? 'has-chat' : ''}`}>
        {isLoading ? (
          <div className="ai-copilot-loading" role="status">
            <span aria-hidden="true" className="material-symbols-outlined">
              progress_activity
            </span>
            {t.aiCopilotLoading}
          </div>
        ) : !canChat ? (
          <section className="ai-copilot-setup" aria-labelledby="ai-copilot-setup-title">
            <span aria-hidden="true" className="material-symbols-outlined ai-copilot-setup-orb">
              auto_awesome
            </span>
            <span className="ai-copilot-eyebrow">{t.aiCopilotPreview}</span>
            <h2 id="ai-copilot-setup-title">{t.aiCopilotNotConfigured}</h2>
            <p>{t.aiCopilotNotConfiguredDescription}</p>
            <button className="ai-copilot-setup-action" type="button" onClick={onOpenSettings}>
              <span aria-hidden="true" className="material-symbols-outlined">
                tune
              </span>
              <span>{t.aiCopilotConfigureProvider}</span>
              <span aria-hidden="true" className="material-symbols-outlined ai-copilot-action-arrow">
                arrow_forward
              </span>
            </button>
            <div className="ai-copilot-principles" aria-label={t.aiCopilotPreview}>
              <article>
                <span aria-hidden="true" className="material-symbols-outlined">
                  shield
                </span>
                <span>
                  <strong>{t.aiCopilotPrivacyTitle}</strong>
                  <small>{t.aiCopilotPrivacyDescription}</small>
                </span>
              </article>
              <article>
                <span aria-hidden="true" className="material-symbols-outlined">
                  terminal
                </span>
                <span>
                  <strong>{t.aiCopilotCommandTitle}</strong>
                  <small>{t.aiCopilotCommandDescription}</small>
                </span>
              </article>
            </div>
          </section>
        ) : (
          <>
            <section aria-label={t.aiCopilotConversationControls} className="ai-copilot-conversation-controls">
              <label>
                <span>{t.aiCopilotProviderLabel}</span>
                <select
                  disabled={isStreaming}
                  value={selectedProviderId ?? ''}
                  onChange={(event) => selectProvider(event.target.value || null)}
                >
                  {providers.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} · {provider.model}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>{t.aiCopilotConversationLabel}</span>
                <select
                  disabled={isStreaming}
                  value={conversation?.id ?? ''}
                  onChange={(event) => {
                    if (event.target.value) {
                      void loadConversation(event.target.value)
                    } else {
                      newChat()
                    }
                  }}
                >
                  <option value="">{t.aiCopilotNewChat}</option>
                  {conversations.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.title}
                    </option>
                  ))}
                </select>
              </label>
            </section>

            <div ref={messageViewportRef} className="ai-copilot-message-viewport" role="log">
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
                    <p>{message.content}</p>
                  </article>
                ))
              )}
              {isStreaming && !conversation?.messages.some((message) => message.role === 'assistant') ? (
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
                    <button disabled={isStreaming} type="button" onClick={() => void retry()}>
                      <span aria-hidden="true" className="material-symbols-outlined">
                        refresh
                      </span>
                      {t.aiCopilotRetry}
                    </button>
                  ) : null}
                </div>
              ) : null}
            </div>
            {usage ? (
              <div className="ai-copilot-usage" aria-label={t.aiCopilotUsage}>
                {t.aiCopilotUsage}: {usage.inputTokens ?? '—'} / {usage.outputTokens ?? '—'}
              </div>
            ) : null}
          </>
        )}
      </div>

      <footer className="ai-copilot-composer-zone">
        <div className={`ai-copilot-composer ${!canChat ? 'is-disabled' : ''}`}>
          <textarea
            aria-label={t.aiCopilotInputAria}
            disabled={!canChat || isStreaming}
            placeholder={canChat ? t.aiCopilotPromptPlaceholder : t.aiCopilotComposerLocked}
            rows={3}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={onComposerKeyDown}
          />
          <div className="ai-copilot-composer-toolbar">
            <span>
              <span aria-hidden="true" className="material-symbols-outlined">
                lock
              </span>
              {isStreaming ? t.aiCopilotThinking : t.aiCopilotL0ComposerHint}
            </span>
            {isStreaming ? (
              <button aria-label={t.aiCopilotStop} className="is-stop" type="button" onClick={() => void stop()}>
                <span aria-hidden="true" className="material-symbols-outlined">
                  stop
                </span>
              </button>
            ) : (
              <button
                aria-label={t.aiCopilotSend}
                disabled={!canChat || !draft.trim()}
                type="button"
                onClick={() => void send()}
              >
                <span aria-hidden="true" className="material-symbols-outlined">
                  arrow_upward
                </span>
              </button>
            )}
          </div>
        </div>
      </footer>
    </aside>
  )
}
