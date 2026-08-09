import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type {
  AiCommandRisk,
  AiCommandSuggestion,
  AiContextTarget,
  AiReviewOutcome,
  ConnectionProfile,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import { CloseButton } from '../common/CloseButton'
import { AiCopilotMarkdown } from './AiCopilotMarkdown'
import { useAiCopilot } from './useAiCopilot'

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

function contextModeLabel(mode: 'metadata' | 'recent-terminal') {
  return mode === 'metadata' ? t.aiCopilotContextMetadata : t.aiCopilotContextRecentTerminal
}

function reviewOutcomeLabel(outcome: AiReviewOutcome) {
  switch (outcome) {
    case 'completed':
      return t.aiCopilotReviewCompleted
    case 'rejected':
      return t.aiCopilotReviewRejected
    case 'approval-dismissed':
      return t.aiCopilotReviewApprovalDismissed
    case 'approval-timed-out':
      return t.aiCopilotReviewApprovalTimedOut
    case 'target-changed':
      return t.aiCopilotReviewTargetChanged
    case 'command-timed-out':
      return t.aiCopilotReviewCommandTimedOut
    case 'failed':
      return t.aiCopilotReviewFailed
  }
}

function reviewTargetLabel(target: AiContextTarget) {
  return target.user ? `${target.user}@${target.displayHost}` : target.displayHost
}

function targetHasChanged({
  target,
  activeProfile,
  activeSession,
  activeTab,
  rootTab
}: {
  target: AiContextTarget
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  rootTab: WorkspaceTab | null
}) {
  if (
    !activeTab ||
    !rootTab ||
    !activeSession ||
    activeTab.id !== target.tabId ||
    rootTab.id !== target.rootTabId ||
    activeTab.sessionType !== target.sessionType ||
    activeSession.connected !== true
  ) {
    return true
  }

  const host = activeSession.accessHost || activeProfile?.host || activeTab.title
  const user = activeSession.shellUser || activeSession.loginUser || activeProfile?.username
  const cwd = activeSession.shellCwd || activeSession.remotePath || undefined
  return (
    (Boolean(target.displayHost) && Boolean(host) && target.displayHost !== host) ||
    (Boolean(target.user) && Boolean(user) && target.user !== user) ||
    (Boolean(target.cwd) && Boolean(cwd) && target.cwd !== cwd)
  )
}

export function AiCopilotPanel({
  activeProfile,
  activeSession,
  activeTab,
  rootTab,
  onClose,
  onOpenSettings
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  rootTab: WorkspaceTab | null
  onClose(): void
  onOpenSettings(): void
}) {
  const isTerminalTarget =
    (activeTab?.sessionType === 'ssh' || activeTab?.sessionType === 'local') && Boolean(activeSession)
  const host = activeSession?.accessHost || activeProfile?.host || activeTab?.title || t.aiCopilotNoTerminalTitle
  const user = activeSession?.shellUser || activeSession?.loginUser || activeProfile?.username
  const targetLabel = isTerminalTarget && user ? `${user}@${host}` : host
  const workingDirectory = activeSession?.shellCwd || activeSession?.remotePath || '~'
  const [draft, setDraft] = useState('')
  const [contextMode, setContextMode] = useState<'metadata' | 'recent-terminal'>('metadata')
  const [commandProposal, setCommandProposal] = useState(false)
  const [commandActionMessage, setCommandActionMessage] = useState<string | null>(null)
  const [writingCommandId, setWritingCommandId] = useState<string | null>(null)
  const [reviewingCommandId, setReviewingCommandId] = useState<string | null>(null)
  const [conversationSearch, setConversationSearch] = useState('')
  const [isRenamingConversation, setIsRenamingConversation] = useState(false)
  const [conversationTitleDraft, setConversationTitleDraft] = useState('')
  const [isDeleteConfirmationOpen, setIsDeleteConfirmationOpen] = useState(false)
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
    contextPreview,
    isContextPreviewing,
    selectProvider,
    loadConversation,
    newChat,
    renameConversation,
    deleteConversation,
    createContextPreview,
    clearContextPreview,
    sendMessage,
    runReview,
    retry,
    stop
  } = useAiCopilot()

  const previewTargetChanged = useMemo(
    () =>
      contextPreview
        ? targetHasChanged({
            target: contextPreview.target,
            activeProfile,
            activeSession,
            activeTab,
            rootTab
          })
        : false,
    [activeProfile, activeSession, activeTab, contextPreview, rootTab]
  )
  const filteredConversations = useMemo(() => {
    const query = conversationSearch.trim().toLocaleLowerCase()
    if (!query) return conversations
    return conversations.filter((item) => item.title.toLocaleLowerCase().includes(query))
  }, [conversationSearch, conversations])

  useEffect(() => {
    const viewport = messageViewportRef.current
    if (viewport) {
      viewport.scrollTop = viewport.scrollHeight
    }
  }, [conversation?.messages, isStreaming])

  useEffect(() => {
    if (!contextPreview) {
      setCommandProposal(false)
    }
  }, [contextPreview])

  useEffect(() => {
    setIsRenamingConversation(false)
    setIsDeleteConfirmationOpen(false)
    setConversationTitleDraft(conversation?.title ?? '')
  }, [conversation?.id, conversation?.title])

  const canChat = Boolean(currentProvider)
  const canRetry = Boolean(errorMessage && conversation?.messages.at(-1)?.role === 'user')

  const send = async () => {
    if (contextPreview && previewTargetChanged) {
      setCommandActionMessage(t.aiCopilotContextTargetChanged)
      return
    }
    const sent = await sendMessage(
      draft,
      contextPreview
        ? {
            contextSnapshotId: contextPreview.snapshotId,
            responseMode: commandProposal ? 'command-proposal' : 'chat'
          }
        : undefined
    )
    if (sent) {
      setDraft('')
    }
  }

  const previewContext = () => {
    if (!currentProvider || !activeTab || !rootTab || !isTerminalTarget) return
    setCommandActionMessage(null)
    void createContextPreview({
      tabId: activeTab.id,
      rootTabId: rootTab.id,
      providerId: currentProvider.id,
      mode: contextMode
    })
  }

  const clearPreview = () => {
    clearContextPreview()
    setCommandProposal(false)
    setCommandActionMessage(null)
  }

  const saveConversationTitle = async () => {
    if (!conversation) return
    const saved = await renameConversation(conversation.id, conversationTitleDraft)
    if (saved) {
      setIsRenamingConversation(false)
      setConversationTitleDraft('')
    }
  }

  const deleteCurrentConversation = async () => {
    if (!conversation) return
    const deleted = await deleteConversation(conversation.id)
    if (deleted) {
      setIsDeleteConfirmationOpen(false)
      setConversationSearch('')
    }
  }

  const copyCommand = async (suggestion: AiCommandSuggestion) => {
    const desktopApi = window.fileterm
    if (!desktopApi) {
      setCommandActionMessage(t.aiCopilotCommandPasteFailed)
      return
    }
    try {
      await desktopApi.writeClipboardText(suggestion.command)
      setCommandActionMessage(t.aiCopilotCommandCopied)
    } catch {
      setCommandActionMessage(t.aiCopilotCommandPasteFailed)
    }
  }

  const writeCommand = async (suggestion: AiCommandSuggestion) => {
    if (suggestion.multiline || suggestion.target.sessionType !== 'ssh') {
      setCommandActionMessage(
        suggestion.multiline ? t.aiCopilotMultilinePasteUnavailable : t.aiCopilotCommandWriteUnavailable
      )
      return
    }
    const desktopApi = window.fileterm
    if (!desktopApi) {
      setCommandActionMessage(t.aiCopilotCommandPasteFailed)
      return
    }
    setWritingCommandId(suggestion.id)
    try {
      const result = await desktopApi.insertAiCommand({ commandId: suggestion.id })
      dispatchAppEvent(APP_EVENT.aiInsertTerminalCommand, { tabId: result.tabId, command: result.command })
      setCommandActionMessage(t.aiCopilotCommandInserted)
    } catch (error) {
      const message = String(error)
        .replace(/^command error:\s*/i, '')
        .replace(/^AI_[A-Z_]+:\s*/i, '')
        .trim()
      setCommandActionMessage(message || t.aiCopilotCommandPasteFailed)
    } finally {
      setWritingCommandId(null)
    }
  }

  const reviewCommand = async (suggestion: AiCommandSuggestion) => {
    const commandTargetChanged = targetHasChanged({
      target: suggestion.target,
      activeProfile,
      activeSession,
      activeTab,
      rootTab
    })
    if (commandTargetChanged) {
      setCommandActionMessage(t.aiCopilotContextTargetChanged)
      return
    }
    if (suggestion.multiline || suggestion.target.sessionType !== 'ssh') {
      setCommandActionMessage(t.aiCopilotReviewUnavailable)
      return
    }
    if (reviewingCommandId) return

    setCommandActionMessage(t.aiCopilotReviewInProgress)
    setReviewingCommandId(suggestion.id)
    try {
      const result = await runReview(suggestion.id)
      if (result) {
        setCommandActionMessage(reviewOutcomeLabel(result.review.outcome))
      } else {
        setCommandActionMessage(t.aiCopilotReviewFailed)
      }
    } finally {
      setReviewingCommandId(null)
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
        <span aria-hidden="true" className={`ai-copilot-target-icon ${isTerminalTarget ? 'is-connected' : ''}`}>
          <span className="material-symbols-outlined">terminal</span>
        </span>
        <span className="ai-copilot-target-copy">
          <strong>{targetLabel}</strong>
          <small>{isTerminalTarget ? workingDirectory : t.aiCopilotNoTerminalDescription}</small>
        </span>
        <span
          className={`ai-copilot-context-status ${
            previewTargetChanged ? 'is-stale' : contextPreview ? 'is-approved' : ''
          }`}
        >
          {previewTargetChanged
            ? t.aiCopilotContextTargetChanged
            : contextPreview
              ? t.aiCopilotContextOn
              : t.aiCopilotContextOff}
        </span>
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
            <div className="ai-copilot-chat-meta">
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
                  <input
                    aria-label={t.aiCopilotHistorySearch}
                    disabled={isStreaming}
                    placeholder={t.aiCopilotHistorySearchPlaceholder}
                    type="search"
                    value={conversationSearch}
                    onChange={(event) => setConversationSearch(event.target.value)}
                  />
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
                    {conversation && !filteredConversations.some((item) => item.id === conversation.id) ? (
                      <option value={conversation.id}>{conversation.title}</option>
                    ) : null}
                    {filteredConversations.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.title}
                      </option>
                    ))}
                  </select>
                </label>
              </section>

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
                    <>
                      <span>{conversation.title}</span>
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
                    </>
                  )
                ) : (
                  <span>{t.aiCopilotNewChat}</span>
                )}
              </section>

              <section className="ai-copilot-context-control" aria-label={t.aiCopilotContextPreview}>
                <div className="ai-copilot-context-control-row">
                  <label>
                    <span>{t.aiCopilotContextPreview}</span>
                    <select
                      disabled={!isTerminalTarget || isStreaming || isContextPreviewing}
                      value={contextMode}
                      onChange={(event) => {
                        setContextMode(event.target.value as 'metadata' | 'recent-terminal')
                        clearPreview()
                      }}
                    >
                      <option value="metadata">{t.aiCopilotContextMetadata}</option>
                      <option value="recent-terminal">{t.aiCopilotContextRecentTerminal}</option>
                    </select>
                  </label>
                  <button
                    disabled={!currentProvider || !isTerminalTarget || isStreaming || isContextPreviewing}
                    type="button"
                    onClick={previewContext}
                  >
                    <span aria-hidden="true" className="material-symbols-outlined">
                      {isContextPreviewing ? 'progress_activity' : 'preview'}
                    </span>
                    {isContextPreviewing ? t.aiCopilotContextPreviewing : t.aiCopilotContextPreview}
                  </button>
                </div>
                {contextPreview ? (
                  <article className="ai-copilot-context-preview">
                    <header>
                      <span>
                        <span aria-hidden="true" className="material-symbols-outlined">
                          verified_user
                        </span>
                        {t.aiCopilotContextPreviewReady} · {contextModeLabel(contextPreview.mode)}
                      </span>
                      <button type="button" onClick={clearPreview}>
                        {t.aiCopilotContextClear}
                      </button>
                    </header>
                    <strong>{t.aiCopilotContextPreviewTitle}</strong>
                    <pre>{contextPreview.preview}</pre>
                    <footer>
                      {previewTargetChanged ? (
                        <span className="is-target-changed">{t.aiCopilotContextTargetChanged}</span>
                      ) : null}
                      {contextPreview.redactions.length
                        ? t.aiCopilotContextRedactions.replace(
                            '{count}',
                            String(contextPreview.redactions.reduce((total, item) => total + item.count, 0))
                          )
                        : null}
                      {contextPreview.truncated ? t.aiCopilotContextTruncated : null}
                    </footer>
                    <small>
                      {previewTargetChanged ? t.aiCopilotContextTargetChangedHint : t.aiCopilotContextPreviewHint}
                    </small>
                  </article>
                ) : (
                  <small className="ai-copilot-context-preview-hint">{t.aiCopilotContextPrototypeHint}</small>
                )}
              </section>
              {commandActionMessage ? (
                <div className="ai-copilot-command-feedback" role="status">
                  {commandActionMessage}
                </div>
              ) : null}
            </div>

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
                    aria-label={
                      message.role === 'user'
                        ? t.aiCopilotMessageUser
                        : message.role === 'review'
                          ? t.aiCopilotMessageReview
                          : t.aiCopilotMessageAssistant
                    }
                  >
                    <span className="ai-copilot-message-role">
                      {message.role === 'user'
                        ? t.aiCopilotMessageUser
                        : message.role === 'review'
                          ? t.aiCopilotMessageReview
                          : t.aiCopilotMessageAssistant}
                    </span>
                    {message.review ? (
                      <section className={`ai-copilot-review-card is-${message.review.outcome}`}>
                        <header>
                          <span className={`ai-copilot-command-risk is-${message.review.risk}`}>
                            {commandRiskLabel(message.review.risk)}
                          </span>
                          <strong>{reviewOutcomeLabel(message.review.outcome)}</strong>
                        </header>
                        <dl>
                          <div>
                            <dt>{t.aiCopilotReviewTarget}</dt>
                            <dd>{reviewTargetLabel(message.review.target)}</dd>
                          </div>
                          <div>
                            <dt>{t.aiCopilotReviewWorkingDirectory}</dt>
                            <dd>{message.review.target.cwd ?? '~'}</dd>
                          </div>
                          <div>
                            <dt>{t.aiCopilotReviewTimeout}</dt>
                            <dd>{Math.ceil(message.review.timeoutMs / 1000)} s</dd>
                          </div>
                          {message.review.exitCode !== undefined ? (
                            <div>
                              <dt>{t.aiCopilotReviewExitCode}</dt>
                              <dd>{message.review.exitCode}</dd>
                            </div>
                          ) : null}
                        </dl>
                        <code>{message.review.command}</code>
                        {message.review.timedOut ? (
                          <p className="is-warning">{t.aiCopilotReviewCommandTimedOut}</p>
                        ) : null}
                        {message.review.outputTruncated ? (
                          <p className="is-warning">{t.aiCopilotReviewOutputTruncated}</p>
                        ) : null}
                        {message.review.error ? <p className="is-error">{message.review.error}</p> : null}
                        {message.review.output ? (
                          <div className="ai-copilot-review-output">
                            <strong>{t.aiCopilotReviewOutput}</strong>
                            <pre>{message.review.output}</pre>
                          </div>
                        ) : message.review.outcome === 'completed' ? (
                          <p>{t.aiCopilotReviewNoOutput}</p>
                        ) : null}
                      </section>
                    ) : message.role === 'assistant' ? (
                      <AiCopilotMarkdown content={message.content} />
                    ) : (
                      <p className="ai-copilot-message-plain">{message.content}</p>
                    )}
                    {message.context ? (
                      <span className="ai-copilot-message-context">
                        <span aria-hidden="true" className="material-symbols-outlined">
                          verified_user
                        </span>
                        {contextModeLabel(message.context.mode)}
                      </span>
                    ) : null}
                    {message.commands?.map((suggestion) => {
                      const commandTargetChanged = targetHasChanged({
                        target: suggestion.target,
                        activeProfile,
                        activeSession,
                        activeTab,
                        rootTab
                      })
                      return (
                        <section key={suggestion.id} className="ai-copilot-command-card">
                          <header>
                            <span className={`ai-copilot-command-risk is-${suggestion.risk}`}>
                              {commandRiskLabel(suggestion.risk)}
                            </span>
                            {commandTargetChanged ? (
                              <span className="is-target-changed">{t.aiCopilotContextTargetChanged}</span>
                            ) : null}
                            {suggestion.multiline ? <span>{t.aiCopilotMultilinePasteUnavailable}</span> : null}
                          </header>
                          <code>{suggestion.command}</code>
                          {suggestion.explanation ? <p>{suggestion.explanation}</p> : null}
                          <footer>
                            <button
                              disabled={
                                suggestion.multiline ||
                                suggestion.target.sessionType !== 'ssh' ||
                                commandTargetChanged ||
                                isStreaming ||
                                reviewingCommandId !== null
                              }
                              title={
                                commandTargetChanged
                                  ? t.aiCopilotContextTargetChangedHint
                                  : suggestion.multiline || suggestion.target.sessionType !== 'ssh'
                                    ? t.aiCopilotReviewUnavailable
                                    : t.aiCopilotReviewCommandHint
                              }
                              type="button"
                              onClick={() => void reviewCommand(suggestion)}
                            >
                              <span aria-hidden="true" className="material-symbols-outlined">
                                {reviewingCommandId === suggestion.id ? 'progress_activity' : 'fact_check'}
                              </span>
                              {t.aiCopilotReviewCommand}
                            </button>
                            <button type="button" onClick={() => void copyCommand(suggestion)}>
                              <span aria-hidden="true" className="material-symbols-outlined">
                                content_copy
                              </span>
                              {t.aiCopilotCopyCommand}
                            </button>
                            <button
                              disabled={
                                suggestion.multiline ||
                                suggestion.target.sessionType !== 'ssh' ||
                                commandTargetChanged ||
                                writingCommandId === suggestion.id
                              }
                              type="button"
                              onClick={() => void writeCommand(suggestion)}
                            >
                              <span aria-hidden="true" className="material-symbols-outlined">
                                input
                              </span>
                              {t.aiCopilotPasteCommand}
                            </button>
                          </footer>
                          <small>
                            {commandTargetChanged
                              ? t.aiCopilotContextTargetChangedHint
                              : `${t.aiCopilotReviewCommandHint} ${t.aiCopilotPasteCommandHint}`}
                          </small>
                        </section>
                      )
                    })}
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
              {isStreaming
                ? t.aiCopilotThinking
                : previewTargetChanged
                  ? t.aiCopilotContextTargetChangedHint
                  : commandProposal
                    ? t.aiCopilotCommandProposalHint
                    : contextPreview
                      ? t.aiCopilotContextPreviewReady
                      : t.aiCopilotL0ComposerHint}
            </span>
            {isStreaming ? (
              <button aria-label={t.aiCopilotStop} className="is-stop" type="button" onClick={() => void stop()}>
                <span aria-hidden="true" className="material-symbols-outlined">
                  stop
                </span>
              </button>
            ) : (
              <>
                <button
                  aria-label={t.aiCopilotCommandProposal}
                  aria-pressed={commandProposal}
                  className={`ai-copilot-command-proposal-toggle ${commandProposal ? 'is-active' : ''}`}
                  disabled={!contextPreview || previewTargetChanged}
                  title={
                    previewTargetChanged
                      ? t.aiCopilotContextTargetChangedHint
                      : contextPreview
                        ? t.aiCopilotCommandProposalHint
                        : t.aiCopilotCommandProposalRequiresContext
                  }
                  type="button"
                  onClick={() => setCommandProposal((current) => !current)}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    terminal
                  </span>
                </button>
                <button
                  aria-label={t.aiCopilotSend}
                  disabled={!canChat || !draft.trim() || (Boolean(contextPreview) && previewTargetChanged)}
                  type="button"
                  onClick={() => void send()}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    arrow_upward
                  </span>
                </button>
              </>
            )}
          </div>
        </div>
      </footer>
    </aside>
  )
}
