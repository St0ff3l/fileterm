import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type {
  AiCopilotMode,
  AiCommandRisk,
  AiCommandSuggestion,
  AiContextTarget,
  AiReviewOutcome,
  ActionApprovalRequest,
  ConnectionProfile,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { AppIcon, type AppIconName } from '../common/AppIcon'
import { DropdownSelect } from '../common/DropdownSelect'
import { VerticalScrollbar } from '../common/VerticalScrollbar'
import { AiCopilotCopyButton } from './AiCopilotCopyButton'
import { AiCopilotMarkdown } from './AiCopilotMarkdown'
import { useAiCopilot, type AiToolActivity } from './useAiCopilot'

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

function copilotModeDescription(mode: AiCopilotMode) {
  switch (mode) {
    case 'pure-conversation':
      return t.aiCopilotModePureDescription
    case 'semi-automatic':
      return t.aiCopilotModeSemiDescription
    case 'fully-automatic':
      return t.aiCopilotModeFullDescription
  }
}

function copilotModeIconName(mode: AiCopilotMode): AppIconName {
  switch (mode) {
    case 'pure-conversation':
      return 'message'
    case 'semi-automatic':
      return 'shield'
    case 'fully-automatic':
      return 'flash'
  }
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

function AiCopilotReviewOutput({ output }: { output: string }) {
  const outputScrollRef = useRef<HTMLPreElement>(null)

  return (
    <div className="ai-copilot-review-output">
      <strong>{t.aiCopilotReviewOutput}</strong>
      <div className="ai-copilot-code-block ai-copilot-review-code-block">
        <pre ref={outputScrollRef}>{output}</pre>
        <AiCopilotCopyButton text={output} />
        <VerticalScrollbar ariaLabel={t.aiCopilotReviewOutput} scrollRef={outputScrollRef} />
      </div>
    </div>
  )
}

function AiCopilotToolActivity({
  activity,
  approval,
  isResolvingApproval = false,
  onResolveApproval
}: {
  activity: AiToolActivity
  approval?: ActionApprovalRequest
  isResolvingApproval?: boolean
  onResolveApproval?: (requestId: string, approved: boolean, riskAcknowledged?: boolean) => void
}) {
  const outputScrollRef = useRef<HTMLPreElement>(null)
  const [riskAcknowledged, setRiskAcknowledged] = useState(false)
  const result = activity.result
  const status = result?.status ?? 'pending'
  const statusLabel = result
    ? status === 'executed'
      ? t.aiCopilotToolExecuted
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
      {result?.reason ? <p className="is-warning">{result.reason}</p> : null}
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
              <input
                checked={riskAcknowledged}
                disabled={isResolvingApproval}
                type="checkbox"
                onChange={(event) => setRiskAcknowledged(event.currentTarget.checked)}
              />
              <span>{t.aiCopilotToolApprovalRisk}</span>
            </label>
          ) : null}
          <footer className="ai-copilot-tool-approval-actions">
            <button
              disabled={isResolvingApproval}
              type="button"
              onClick={() => onResolveApproval?.(approval.requestId, false)}
            >
              <AppIcon name="close" size={13} />
              {t.aiCopilotToolReject}
            </button>
            <button
              disabled={
                isResolvingApproval || (approval.requiresRiskAcknowledgement && !riskAcknowledged) || !onResolveApproval
              }
              type="button"
              onClick={() => onResolveApproval?.(approval.requestId, true, riskAcknowledged)}
            >
              <AppIcon name="check" size={13} />
              {t.aiCopilotToolApprove}
            </button>
          </footer>
        </div>
      ) : null}
    </section>
  )
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
    (Boolean(target.sessionRevision) && target.sessionRevision !== activeSession.aiSessionRevision) ||
    target.displayHost !== host ||
    (target.user ?? undefined) !== (user ?? undefined) ||
    (target.cwd ?? undefined) !== (cwd ?? undefined)
  )
}

export function AiCopilotPanel({
  activeProfile,
  activeSession,
  activeTab,
  rootTab,
  isResizing,
  onClose,
  onOpenSettings,
  onResizeStart
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  rootTab: WorkspaceTab | null
  isResizing?: boolean
  onClose(): void
  onOpenSettings(): void
  onResizeStart?: () => void
}) {
  const isTerminalTarget =
    (activeTab?.sessionType === 'ssh' || activeTab?.sessionType === 'local') && activeSession?.connected === true
  const [draft, setDraft] = useState('')
  const [referenceTerminal, setReferenceTerminal] = useState(false)
  const [isAutoModeConfirmOpen, setIsAutoModeConfirmOpen] = useState(false)
  const [isAutoModeConfirming, setIsAutoModeConfirming] = useState(false)
  const [commandActionMessage, setCommandActionMessage] = useState<string | null>(null)
  const [writingCommandIds, setWritingCommandIds] = useState<Set<string>>(() => new Set())
  const writingCommandIdsRef = useRef<Set<string>>(new Set())
  const [reviewingCommandIds, setReviewingCommandIds] = useState<Set<string>>(() => new Set())
  const reviewingCommandIdsRef = useRef<Set<string>>(new Set())
  const [conversationSearch, setConversationSearch] = useState('')
  const [isConversationListOpen, setIsConversationListOpen] = useState(false)
  const [isRenamingConversation, setIsRenamingConversation] = useState(false)
  const [conversationTitleDraft, setConversationTitleDraft] = useState('')
  const [isDeleteConfirmationOpen, setIsDeleteConfirmationOpen] = useState(false)
  const composerCompositionRef = useRef(false)
  const panelRef = useRef<HTMLElement>(null)
  const messageViewportRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handleSelectionChange = () => {
      const selection = window.getSelection()
      if (!selection || selection.isCollapsed || !selection.anchorNode) return

      const panel = panelRef.current
      if (!panel) return

      const isAnchorInside = panel.contains(selection.anchorNode)
      const isFocusInside = selection.focusNode ? panel.contains(selection.focusNode) : false

      if (isAnchorInside && !isFocusInside) {
        const range = selection.getRangeAt(0)
        const treeWalker = document.createTreeWalker(panel, NodeFilter.SHOW_TEXT)
        let lastTextNode: Node | null = null
        while (treeWalker.nextNode()) {
          lastTextNode = treeWalker.currentNode
        }
        if (lastTextNode) {
          try {
            range.setEnd(lastTextNode, lastTextNode.textContent?.length ?? 0)
            selection.removeAllRanges()
            selection.addRange(range)
          } catch {
            selection.collapseToStart()
          }
        } else {
          selection.collapseToStart()
        }
      }
    }

    document.addEventListener('selectionchange', handleSelectionChange)
    return () => {
      document.removeEventListener('selectionchange', handleSelectionChange)
    }
  }, [])
  const {
    providers,
    conversations,
    conversation,
    currentProvider,
    selectedProviderId,
    selectedModel,
    isLoading,
    isStreaming,
    errorMessage,
    usage,
    toolActivities,
    toolApprovalRequests,
    resolvingToolApprovalIds,
    isContextPreviewing,
    modeState,
    selectProvider,
    selectModel,
    loadConversation,
    newChat,
    renameConversation,
    deleteConversation,
    createContextPreview,
    sendMessage,
    runReview,
    setCopilotMode,
    setContextAttach,
    setDangerousCommandRestrictions,
    resolveToolApproval,
    retry,
    stop
  } = useAiCopilot()

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
    setIsRenamingConversation(false)
    setIsDeleteConfirmationOpen(false)
    setConversationTitleDraft(conversation?.title ?? '')
  }, [conversation?.id, conversation?.title])

  useEffect(() => {
    if (!modeState) return
    setReferenceTerminal(modeState.attachTerminalContext)
  }, [modeState])

  const canChat = Boolean(currentProvider)
  const copilotMode: AiCopilotMode = modeState?.mode ?? 'pure-conversation'
  const requiresTerminalContext = copilotMode !== 'pure-conversation' || referenceTerminal
  const canRetry = Boolean(errorMessage && conversation?.messages.at(-1)?.role === 'user')

  const openNewConversation = () => {
    if (isStreaming) return
    newChat()
    setIsConversationListOpen(false)
  }

  const send = async () => {
    if (!draft.trim() || isStreaming) return
    const shouldAttachContext = copilotMode !== 'pure-conversation' || referenceTerminal

    let contextSnapshot: Awaited<ReturnType<typeof createContextPreview>> = null
    if (shouldAttachContext) {
      if (!currentProvider || !activeTab || !rootTab || !isTerminalTarget) {
        setCommandActionMessage(t.aiCopilotContextUnavailable)
        return
      }
      setCommandActionMessage(null)
      contextSnapshot = await createContextPreview({
        tabId: activeTab.id,
        rootTabId: rootTab.id,
        providerId: currentProvider.id,
        mode: 'L2'
      })
      if (!contextSnapshot) return
    }

    const sent = await sendMessage(draft, {
      mode: copilotMode,
      ...(contextSnapshot
        ? {
            contextSnapshotId: contextSnapshot.snapshotId,
            contextPreview: contextSnapshot
          }
        : {})
    })
    if (sent) {
      setDraft('')
    }
  }

  const retryLastRequest = async () => {
    if (isStreaming) return
    const shouldAttachContext = copilotMode !== 'pure-conversation' || referenceTerminal
    let contextSnapshot: Awaited<ReturnType<typeof createContextPreview>> = null
    if (shouldAttachContext) {
      if (!currentProvider || !activeTab || !rootTab || !isTerminalTarget) {
        setCommandActionMessage(t.aiCopilotContextUnavailable)
        return
      }
      setCommandActionMessage(null)
      contextSnapshot = await createContextPreview({
        tabId: activeTab.id,
        rootTabId: rootTab.id,
        providerId: currentProvider.id,
        mode: 'L2'
      })
      if (!contextSnapshot) return
    }
    await retry({
      mode: copilotMode,
      ...(contextSnapshot ? { contextSnapshotId: contextSnapshot.snapshotId } : {})
    })
  }

  const toggleTerminalReference = () => {
    if (isStreaming || isContextPreviewing) return
    setCommandActionMessage(null)
    const next = !referenceTerminal
    setReferenceTerminal(next)
    void setContextAttach(next).then((state) => {
      if (!state) setReferenceTerminal(referenceTerminal)
    })
  }

  const selectCopilotMode = (nextMode: AiCopilotMode) => {
    if (isStreaming || nextMode === copilotMode) return
    if (nextMode === 'fully-automatic') {
      setIsAutoModeConfirmOpen(true)
      return
    }
    void setCopilotMode(nextMode)
  }

  const confirmFullyAutomaticMode = async () => {
    setIsAutoModeConfirming(true)
    const next = await setCopilotMode('fully-automatic', true)
    setIsAutoModeConfirming(false)
    if (next) setIsAutoModeConfirmOpen(false)
  }

  const dangerousCommandRestrictionsEnabled = modeState?.autoModeGuardrails.dangerousCommandRestrictionsEnabled ?? true

  const toggleDangerousCommandRestrictions = () => {
    if (isStreaming || !modeState) return
    void setDangerousCommandRestrictions(!dangerousCommandRestrictionsEnabled)
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

  const writeCommand = async (suggestion: AiCommandSuggestion) => {
    if (suggestion.multiline || suggestion.target.sessionType !== 'ssh') {
      setCommandActionMessage(
        suggestion.multiline ? t.aiCopilotMultilinePasteUnavailable : t.aiCopilotCommandWriteUnavailable
      )
      return
    }
    const desktopApi = window.fileterm
    if (!desktopApi) {
      setCommandActionMessage(t.aiCopilotTerminalInputWriteFailed)
      return
    }
    if (writingCommandIdsRef.current.has(suggestion.id)) return
    writingCommandIdsRef.current.add(suggestion.id)
    setWritingCommandIds(new Set(writingCommandIdsRef.current))
    try {
      const result = await desktopApi.insertAiCommand({ commandId: suggestion.id })
      dispatchAppEvent(APP_EVENT.aiInsertTerminalCommand, { tabId: result.tabId, command: result.command })
      setCommandActionMessage(t.aiCopilotTerminalInputWritten)
    } catch (error) {
      const message = String(error)
        .replace(/^command error:\s*/i, '')
        .replace(/^AI_[A-Z_]+:\s*/i, '')
        .trim()
      setCommandActionMessage(message || t.aiCopilotTerminalInputWriteFailed)
    } finally {
      writingCommandIdsRef.current.delete(suggestion.id)
      setWritingCommandIds(new Set(writingCommandIdsRef.current))
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
    if (reviewingCommandIdsRef.current.has(suggestion.id)) return

    setCommandActionMessage(t.aiCopilotReviewInProgress)
    reviewingCommandIdsRef.current.add(suggestion.id)
    setReviewingCommandIds(new Set(reviewingCommandIdsRef.current))
    try {
      const result = await runReview(suggestion.id)
      if (result) {
        // The completed state is already rendered in the persisted review
        // record. Do not repeat “Ran once” above the composer.
        setCommandActionMessage(
          result.review.outcome === 'completed' ? null : reviewOutcomeLabel(result.review.outcome)
        )
      } else {
        setCommandActionMessage(t.aiCopilotReviewFailed)
      }
    } finally {
      reviewingCommandIdsRef.current.delete(suggestion.id)
      setReviewingCommandIds(new Set(reviewingCommandIdsRef.current))
    }
  }

  const onComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    // Some macOS input methods report `isComposing` as false on the Enter
    // keydown that commits a candidate. keyCode 229 and the explicit
    // composition ref cover both browser event orderings.
    if (
      event.key !== 'Enter' ||
      event.nativeEvent.isComposing ||
      event.keyCode === 229 ||
      composerCompositionRef.current ||
      event.shiftKey
    ) {
      return
    }
    event.preventDefault()
    void send()
  }

  return (
    <aside
      ref={panelRef}
      aria-label={t.aiCopilot}
      className={`ai-copilot-panel ${isResizing ? 'is-resizing' : ''}`}
      onMouseDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <div
        aria-label="调整 AI 侧边栏宽度"
        className={`ai-copilot-resizer ${isResizing ? 'is-active' : ''}`}
        onMouseDown={(event) => {
          event.preventDefault()
          event.stopPropagation()
          onResizeStart?.()
        }}
        role="separator"
      />
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

      <div
        className={`ai-copilot-content ${canChat ? 'has-chat' : ''} ${isConversationListOpen ? 'is-conversation-list' : ''}`}
      >
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
        ) : isConversationListOpen ? (
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
                  <button
                    key={item.id}
                    className={`ai-copilot-conversation-list-item ${item.id === conversation?.id ? 'is-active' : ''}`}
                    disabled={isStreaming}
                    role="listitem"
                    type="button"
                    onClick={() => {
                      setIsConversationListOpen(false)
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
        ) : (
          <>
            <div className="ai-copilot-chat-meta">
              <div className="ai-copilot-conversation-page-header">
                <div className="ai-copilot-conversation-page-heading">
                  <button
                    aria-label={t.aiCopilotConversationBack}
                    className="ai-copilot-conversation-back"
                    disabled={isStreaming}
                    title={t.aiCopilotConversationBack}
                    type="button"
                    onClick={() => setIsConversationListOpen(true)}
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
            </div>

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
                          <div className="ai-copilot-code-block ai-copilot-review-code-block">
                            <code>{message.review.command}</code>
                            <AiCopilotCopyButton text={message.review.command} />
                          </div>
                          {message.review.timedOut ? (
                            <p className="is-warning">{t.aiCopilotReviewCommandTimedOut}</p>
                          ) : null}
                          {message.review.outputTruncated ? (
                            <p className="is-warning">{t.aiCopilotReviewOutputTruncated}</p>
                          ) : null}
                          {message.review.error ? <p className="is-error">{message.review.error}</p> : null}
                          {message.review.output ? (
                            <AiCopilotReviewOutput output={message.review.output} />
                          ) : message.review.outcome === 'completed' ? (
                            <p>{t.aiCopilotReviewNoOutput}</p>
                          ) : null}
                        </section>
                      ) : message.role === 'assistant' ? (
                        <AiCopilotMarkdown content={message.content} />
                      ) : (
                        <p className="ai-copilot-message-plain">{message.content}</p>
                      )}
                      {message.commands?.map((suggestion) => {
                        const commandTargetChanged = targetHasChanged({
                          target: suggestion.target,
                          activeProfile,
                          activeSession,
                          activeTab,
                          rootTab
                        })
                        const commandIsBusy =
                          reviewingCommandIds.has(suggestion.id) || writingCommandIds.has(suggestion.id)
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
                            <div className="ai-copilot-code-block ai-copilot-command-code-block">
                              <code>{suggestion.command}</code>
                              <AiCopilotCopyButton text={suggestion.command} />
                            </div>
                            {suggestion.explanation ? <p>{suggestion.explanation}</p> : null}
                            <footer>
                              <button
                                disabled={
                                  suggestion.multiline ||
                                  suggestion.target.sessionType !== 'ssh' ||
                                  commandTargetChanged ||
                                  isStreaming ||
                                  commandIsBusy
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
                                  {reviewingCommandIds.has(suggestion.id) ? 'progress_activity' : 'fact_check'}
                                </span>
                                {t.aiCopilotReviewCommand}
                              </button>
                              <button
                                disabled={
                                  suggestion.multiline ||
                                  suggestion.target.sessionType !== 'ssh' ||
                                  commandTargetChanged ||
                                  commandIsBusy
                                }
                                title={t.aiCopilotWriteTerminalInputHint}
                                type="button"
                                onClick={() => void writeCommand(suggestion)}
                              >
                                <span aria-hidden="true" className="material-symbols-outlined">
                                  input
                                </span>
                                {t.aiCopilotWriteTerminalInput}
                              </button>
                            </footer>
                            <small>
                              {commandTargetChanged
                                ? t.aiCopilotContextTargetChangedHint
                                : `${t.aiCopilotReviewCommandHint} ${t.aiCopilotWriteTerminalInputHint}`}
                            </small>
                          </section>
                        )
                      })}
                    </article>
                  ))
                )}
                {toolActivities.length > 0 ? (
                  <div className="ai-copilot-tool-activities" aria-label={t.aiCopilotToolActivity}>
                    {toolActivities.map((activity) => (
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
                      />
                    ))}
                  </div>
                ) : null}
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
                      <button disabled={isStreaming} type="button" onClick={() => void retryLastRequest()}>
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
            {usage ? (
              <div className="ai-copilot-usage" aria-label={t.aiCopilotUsage}>
                {t.aiCopilotUsage}: {usage.inputTokens ?? '—'} / {usage.outputTokens ?? '—'}
              </div>
            ) : null}
          </>
        )}
      </div>

      {!isConversationListOpen ? (
        <footer className="ai-copilot-composer-area">
          <div className="ai-copilot-composer-zone">
            {canChat ? (
              <section className="ai-copilot-context-dock" aria-label={t.aiCopilotReferenceTerminal}>
                <div className="ai-copilot-context-dock-row">
                  <button
                    aria-pressed={referenceTerminal}
                    aria-label={t.aiCopilotReferenceTerminal}
                    className={`ai-copilot-context-switch ${referenceTerminal ? 'is-active' : ''}`}
                    disabled={
                      isStreaming ||
                      isContextPreviewing ||
                      copilotMode !== 'pure-conversation' ||
                      (!isTerminalTarget && !referenceTerminal)
                    }
                    type="button"
                    onClick={toggleTerminalReference}
                  >
                    <span aria-hidden="true" className="material-symbols-outlined">
                      {referenceTerminal ? 'visibility' : 'visibility_off'}
                    </span>
                    <span>{t.aiCopilotReferenceTerminal}</span>
                    <span className="ai-copilot-context-switch-state">
                      {isContextPreviewing
                        ? t.aiCopilotContextPreparing
                        : referenceTerminal
                          ? t.aiCopilotReferenceTerminalOn
                          : t.aiCopilotReferenceTerminalOff}
                    </span>
                  </button>
                  <span className="ai-copilot-context-dock-hint">
                    {copilotMode !== 'pure-conversation'
                      ? t.aiCopilotContextLockedByMode
                      : referenceTerminal
                        ? isTerminalTarget
                          ? t.aiCopilotContextAutoHint
                          : t.aiCopilotContextNeedsTerminal
                        : t.aiCopilotL0ComposerHint}
                  </span>
                </div>
                {commandActionMessage ? (
                  <div className="ai-copilot-command-feedback" role="status">
                    {commandActionMessage}
                  </div>
                ) : null}
              </section>
            ) : null}
            <div className={`ai-copilot-composer ${!canChat ? 'is-disabled' : ''}`}>
              <textarea
                aria-label={t.aiCopilotInputAria}
                disabled={!canChat || isStreaming}
                placeholder={canChat ? t.aiCopilotPromptPlaceholder : t.aiCopilotComposerLocked}
                rows={3}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onCompositionEnd={() => {
                  // Keep the guard active through the IME commit keydown.
                  window.setTimeout(() => {
                    composerCompositionRef.current = false
                  }, 0)
                }}
                onCompositionStart={() => {
                  composerCompositionRef.current = true
                }}
                onKeyDown={onComposerKeyDown}
              />
              <div className="ai-copilot-composer-toolbar">
                <div className="ai-copilot-composer-toolbar-actions">
                  {canChat ? (
                    <div className="ai-copilot-composer-models" aria-label={t.aiCopilotConversationControls}>
                      <DropdownSelect
                        className="ai-copilot-composer-select ai-copilot-provider-select"
                        disabled={isStreaming}
                        options={providers.map((provider) => ({ value: provider.id, label: provider.name }))}
                        value={selectedProviderId ?? ''}
                        onChange={(value) => selectProvider(value || null)}
                      />
                      {currentProvider ? (
                        <>
                          <span aria-hidden="true" className="ai-copilot-composer-model-divider" />
                          <DropdownSelect
                            className="ai-copilot-composer-select ai-copilot-model-select"
                            disabled={isStreaming}
                            options={(currentProvider.models && currentProvider.models.length > 0
                              ? currentProvider.models
                              : [currentProvider.model]
                            ).map((model) => ({ value: model, label: model }))}
                            value={selectedModel ?? currentProvider.model}
                            onChange={(value) => selectModel(value || null)}
                          />
                          <span
                            aria-hidden="true"
                            className="ai-copilot-composer-model-divider ai-copilot-mode-divider"
                          />
                          <DropdownSelect
                            ariaLabel={t.aiCopilotModeLabel}
                            className="ai-copilot-composer-select ai-copilot-mode-select"
                            disabled={isStreaming}
                            forceCustomMenu
                            menuClassName="ai-copilot-mode-menu"
                            menuPlacement="above"
                            menuWidth="auto"
                            options={[
                              { value: 'pure-conversation', label: t.aiCopilotModePure },
                              { value: 'semi-automatic', label: t.aiCopilotModeSemi },
                              { value: 'fully-automatic', label: t.aiCopilotModeFull }
                            ]}
                            renderOption={(option, selected) => {
                              const optionMode = option.value as AiCopilotMode
                              return (
                                <span className="ai-copilot-mode-option">
                                  <span className="ai-copilot-mode-option-copy">
                                    <strong>
                                      <AppIcon
                                        className="ai-copilot-mode-option-icon"
                                        name={copilotModeIconName(optionMode)}
                                        size={12}
                                      />
                                      <span>{option.label}</span>
                                      {selected ? (
                                        <AppIcon className="ai-copilot-mode-option-check" name="check" size={13} />
                                      ) : null}
                                    </strong>
                                    <small>{copilotModeDescription(optionMode)}</small>
                                  </span>
                                </span>
                              )
                            }}
                            renderValue={(option) => {
                              const optionMode = option.value as AiCopilotMode
                              return (
                                <span className="ai-copilot-mode-value">
                                  <AppIcon
                                    className="ai-copilot-mode-value-icon"
                                    name={copilotModeIconName(optionMode)}
                                    size={10}
                                  />
                                  <span>{option.label}</span>
                                </span>
                              )
                            }}
                            value={copilotMode}
                            onChange={(value) => selectCopilotMode(value as AiCopilotMode)}
                          />
                        </>
                      ) : null}
                    </div>
                  ) : null}
                  {isStreaming ? (
                    <button
                      aria-label={t.aiCopilotStop}
                      className="ai-copilot-composer-send is-stop"
                      type="button"
                      onClick={() => void stop()}
                    >
                      <span aria-hidden="true" className="material-symbols-outlined">
                        stop
                      </span>
                    </button>
                  ) : (
                    <button
                      aria-label={t.aiCopilotSend}
                      className="ai-copilot-composer-send"
                      disabled={
                        !canChat ||
                        !draft.trim() ||
                        (requiresTerminalContext && (!referenceTerminal || !isTerminalTarget))
                      }
                      type="button"
                      onClick={() => void send()}
                    >
                      <AppIcon name="arrow-up" size={16} />
                    </button>
                  )}
                </div>
              </div>
            </div>
          </div>
        </footer>
      ) : null}
      {!isConversationListOpen ? (
        <section
          aria-label={copilotMode === 'fully-automatic' ? t.aiCopilotDangerousCommandRestrictions : undefined}
          className="ai-copilot-dangerous-command-dock"
        >
          {copilotMode === 'fully-automatic' ? (
            <>
              <span className="ai-copilot-dangerous-command-dock-hint">
                {t.aiCopilotDangerousCommandRestrictionsHint}
              </span>
              <button
                aria-checked={dangerousCommandRestrictionsEnabled}
                aria-label={`${t.aiCopilotDangerousCommandRestrictions} · ${
                  dangerousCommandRestrictionsEnabled
                    ? t.aiCopilotDangerousCommandRestrictionsOn
                    : t.aiCopilotDangerousCommandRestrictionsOff
                }`}
                className={`ai-copilot-dangerous-command-toggle ${
                  dangerousCommandRestrictionsEnabled ? 'is-enabled' : 'is-disabled'
                }`}
                disabled={isStreaming || !modeState}
                role="switch"
                title={t.aiCopilotDangerousCommandRestrictionsDescription}
                type="button"
                onClick={toggleDangerousCommandRestrictions}
              >
                <AppIcon name={dangerousCommandRestrictionsEnabled ? 'shield-check' : 'shield'} size={12} />
                <span>{t.aiCopilotDangerousCommandRestrictions}</span>
                <strong>
                  {dangerousCommandRestrictionsEnabled
                    ? t.aiCopilotDangerousCommandRestrictionsOn
                    : t.aiCopilotDangerousCommandRestrictionsOff}
                </strong>
              </button>
            </>
          ) : null}
        </section>
      ) : null}
      {isAutoModeConfirmOpen ? (
        <ConfirmActionDialog
          confirmLabel={t.aiCopilotModeFullConfirm}
          confirmVariant="danger"
          description={t.aiCopilotModeFullWarning}
          isSubmitting={isAutoModeConfirming}
          onClose={() => {
            if (!isAutoModeConfirming) setIsAutoModeConfirmOpen(false)
          }}
          onConfirm={() => void confirmFullyAutomaticMode()}
          title={t.aiCopilotModeFullTitle}
        />
      ) : null}
    </aside>
  )
}
