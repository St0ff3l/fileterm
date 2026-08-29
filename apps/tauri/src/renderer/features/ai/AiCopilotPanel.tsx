import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type {
  AiCopilotMode,
  AiCommandRisk,
  AiToolActivity,
  AiConversationSummary,
  AiMessage,
  ActionApprovalRequest,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { AppIcon, type AppIconName } from '../common/AppIcon'
import { DropdownSelect } from '../common/DropdownSelect'
import { SelectionControl } from '../common/SelectionControl'
import { StableButtonContent, StableButtonLabel } from '../common/StableButtonContent'
import { VerticalScrollbar } from '../common/VerticalScrollbar'
import { AiCopilotCopyButton } from './AiCopilotCopyButton'
import { AiCopilotMarkdown } from './AiCopilotMarkdown'
import { AiCopilotMessageActions } from './AiCopilotMessageActions'
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

const COMMAND_FEEDBACK_DISMISS_MS = 4_000

function AiCopilotToolActivity({
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
  const canExecuteTerminalCommand = Boolean(
    onExecuteTerminalCommand && !/[\r\n]/.test(activity.proposal.command) && !isExecutingTerminalCommand
  )
  const executeTerminalCommandButton =
    onExecuteTerminalCommand && !result ? (
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

export function AiCopilotPanel({
  activeSession,
  activeTab,
  rootTab,
  isResizing,
  onClose,
  onOpenSettings,
  onResizeStart
}: {
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
  const [conversationSearch, setConversationSearch] = useState('')
  const [isConversationListOpen, setIsConversationListOpen] = useState(false)
  const [isRenamingConversation, setIsRenamingConversation] = useState(false)
  const [conversationTitleDraft, setConversationTitleDraft] = useState('')
  const [isDeleteConfirmationOpen, setIsDeleteConfirmationOpen] = useState(false)
  const [conversationPendingDeletion, setConversationPendingDeletion] = useState<AiConversationSummary | null>(null)
  const [messagePendingDeletion, setMessagePendingDeletion] = useState<AiMessage | null>(null)
  const [isDeletingConversation, setIsDeletingConversation] = useState(false)
  const [isDeletingMessage, setIsDeletingMessage] = useState(false)
  const composerCompositionRef = useRef(false)
  const panelRef = useRef<HTMLElement>(null)
  const messageViewportRef = useRef<HTMLDivElement>(null)
  const commandFeedbackTimerRef = useRef<number | null>(null)

  const showCommandActionMessage = (message: string) => {
    if (commandFeedbackTimerRef.current !== null) {
      window.clearTimeout(commandFeedbackTimerRef.current)
    }
    setCommandActionMessage(message)
    commandFeedbackTimerRef.current = window.setTimeout(() => {
      setCommandActionMessage(null)
      commandFeedbackTimerRef.current = null
    }, COMMAND_FEEDBACK_DISMISS_MS)
  }

  useEffect(() => {
    return () => {
      if (commandFeedbackTimerRef.current !== null) {
        window.clearTimeout(commandFeedbackTimerRef.current)
      }
    }
  }, [])

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
    deleteMessage,
    deleteConversation,
    createContextPreview,
    sendMessage,
    setCopilotMode,
    setContextAttach,
    setDangerousCommandRestrictions,
    resolveToolApproval,
    resolveToolApprovalAsTerminal,
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
    if (!viewport) return

    // Text and tool results arrive in separate render passes. Defer the final
    // scroll until the new message/card has been laid out so the completed
    // summary is visible instead of leaving the viewport on the old bubble.
    const frame = window.requestAnimationFrame(() => {
      if (messageViewportRef.current === viewport) {
        viewport.scrollTop = viewport.scrollHeight
      }
    })
    return () => window.cancelAnimationFrame(frame)
  }, [conversation?.messages, isStreaming, toolActivities])

  useEffect(() => {
    setIsRenamingConversation(false)
    setIsDeleteConfirmationOpen(false)
    setMessagePendingDeletion(null)
    setConversationTitleDraft(conversation?.title ?? '')
  }, [conversation?.id, conversation?.title])

  useEffect(() => {
    if (!modeState) return
    setReferenceTerminal(modeState.attachTerminalContext)
  }, [modeState])

  const canChat = Boolean(currentProvider)
  const copilotMode: AiCopilotMode = modeState?.mode ?? 'pure-conversation'
  const showsDangerousCommandRestrictions = copilotMode !== 'pure-conversation'
  const requiresTerminalContext = copilotMode !== 'pure-conversation' || referenceTerminal
  const canRetry = Boolean(errorMessage && conversation?.messages.at(-1)?.role === 'user')

  const openNewConversation = () => {
    if (isStreaming) return
    newChat()
    setDraft('')
    setConversationSearch('')
    setIsConversationListOpen(false)
  }

  const send = async () => {
    if (!draft.trim() || isStreaming) return
    const shouldAttachContext = copilotMode !== 'pure-conversation' || referenceTerminal

    let contextSnapshot: Awaited<ReturnType<typeof createContextPreview>> = null
    if (shouldAttachContext) {
      if (!currentProvider || !activeTab || !rootTab || !isTerminalTarget) {
        showCommandActionMessage(t.aiCopilotContextUnavailable)
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
        showCommandActionMessage(t.aiCopilotContextUnavailable)
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

  const executeCommandInTerminal = (activity: AiToolActivity): Promise<void> => {
    const targetTabId = activity.proposal.target.tabId
    if (
      !window.fileterm ||
      !activeTab ||
      activeTab.id !== targetTabId ||
      activeTab.sessionType !== 'ssh' ||
      !isTerminalTarget ||
      /[\r\n]/.test(activity.proposal.command)
    ) {
      showCommandActionMessage(t.aiCopilotCommandWriteUnavailable)
      return Promise.reject(new Error(t.aiCopilotCommandWriteUnavailable))
    }

    return new Promise<void>((resolve, reject) => {
      dispatchAppEvent(APP_EVENT.aiInsertTerminalCommand, {
        tabId: targetTabId,
        command: activity.proposal.command,
        execute: true,
        onComplete: () => {
          const approvalRequestId = activity.proposal.approvalRequestId
          const handoff = approvalRequestId ? resolveToolApprovalAsTerminal(approvalRequestId) : Promise.resolve()
          void handoff
            .then(() => {
              showCommandActionMessage(t.aiCopilotTerminalInputWritten)
              resolve()
            })
            .catch((error) => {
              showCommandActionMessage(t.aiCopilotTerminalInputWriteFailed)
              reject(error)
            })
        },
        onError: () => {
          showCommandActionMessage(t.aiCopilotTerminalInputWriteFailed)
          reject(new Error(t.aiCopilotTerminalInputWriteFailed))
        }
      })
    })
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

  const confirmConversationDeletion = async () => {
    if (!conversationPendingDeletion) return
    setIsDeletingConversation(true)
    const deleted = await deleteConversation(conversationPendingDeletion.id)
    setIsDeletingConversation(false)
    if (deleted) {
      setConversationPendingDeletion(null)
      setConversationSearch('')
    }
  }

  const confirmMessageDeletion = async () => {
    if (!messagePendingDeletion || !conversation || isStreaming) return
    setIsDeletingMessage(true)
    const deleted = await deleteMessage(conversation.id, messagePendingDeletion.id)
    setIsDeletingMessage(false)
    if (deleted) {
      setMessagePendingDeletion(null)
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
                    <button
                      aria-label={`${t.aiCopilotDeleteConversation}: ${item.title}`}
                      className="ai-copilot-conversation-list-item-delete"
                      disabled={isStreaming && item.id === conversation?.id}
                      title={t.aiCopilotDeleteConversation}
                      type="button"
                      onClick={() => setConversationPendingDeletion(item)}
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
              {commandActionMessage ? (
                <div className="ai-copilot-command-feedback" role="status" aria-live="polite">
                  {commandActionMessage}
                </div>
              ) : null}
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
                          if (!isStreaming) setMessagePendingDeletion(message)
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
                    aria-busy={isContextPreviewing}
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
                    <span aria-hidden="true" className="stable-button-icon ai-copilot-context-switch-icon">
                      {isContextPreviewing ? (
                        <span className="button-spinner" />
                      ) : (
                        <span className="material-symbols-outlined">
                          {referenceTerminal ? 'visibility' : 'visibility_off'}
                        </span>
                      )}
                    </span>
                    <span>{t.aiCopilotReferenceTerminal}</span>
                    <span className="ai-copilot-context-switch-state">
                      <StableButtonLabel
                        busy={isContextPreviewing}
                        busyLabel={t.aiCopilotContextPreparing}
                        label={referenceTerminal ? t.aiCopilotReferenceTerminalOn : t.aiCopilotReferenceTerminalOff}
                        reserveLabel={
                          referenceTerminal ? t.aiCopilotReferenceTerminalOff : t.aiCopilotReferenceTerminalOn
                        }
                      />
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
          aria-label={showsDangerousCommandRestrictions ? t.aiCopilotDangerousCommandRestrictions : undefined}
          className="ai-copilot-dangerous-command-dock"
        >
          {showsDangerousCommandRestrictions ? (
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
                <StableButtonLabel
                  label={
                    <strong>
                      {dangerousCommandRestrictionsEnabled
                        ? t.aiCopilotDangerousCommandRestrictionsOn
                        : t.aiCopilotDangerousCommandRestrictionsOff}
                    </strong>
                  }
                  reserveLabel={
                    <strong>
                      {dangerousCommandRestrictionsEnabled
                        ? t.aiCopilotDangerousCommandRestrictionsOff
                        : t.aiCopilotDangerousCommandRestrictionsOn}
                    </strong>
                  }
                />
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
      {conversationPendingDeletion ? (
        <ConfirmActionDialog
          confirmLabel={t.delete}
          confirmVariant="danger"
          description={t.aiCopilotDeleteConversationConfirm}
          errorMessage={errorMessage}
          isSubmitting={isDeletingConversation}
          onClose={() => {
            if (!isDeletingConversation) setConversationPendingDeletion(null)
          }}
          onConfirm={() => void confirmConversationDeletion()}
          title={t.aiCopilotDeleteConversation}
        />
      ) : null}
      {messagePendingDeletion ? (
        <ConfirmActionDialog
          confirmLabel={t.aiCopilotDeleteMessage}
          confirmVariant="danger"
          description={t.aiCopilotDeleteMessageConfirm}
          errorMessage={errorMessage}
          isSubmitting={isDeletingMessage}
          onClose={() => {
            if (!isDeletingMessage) setMessagePendingDeletion(null)
          }}
          onConfirm={() => void confirmMessageDeletion()}
          title={t.aiCopilotDeleteMessage}
        />
      ) : null}
    </aside>
  )
}
