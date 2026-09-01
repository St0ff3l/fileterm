import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type {
  AiCopilotMode,
  AiConversationSummary,
  AiMessage,
  AiToolActivity,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { type AppIconName } from '../common/app-icon'
import { useAiCopilot } from './use-ai-copilot'
import { AiCopilotChatMeta } from './ai-copilot-chat-meta'
import { AiCopilotComposer } from './ai-copilot-composer'
import { AiCopilotConversationList } from './ai-copilot-conversation-list'
import { AiCopilotDangerousDock } from './ai-copilot-dangerous-dock'
import { AiCopilotHeader } from './ai-copilot-header'
import { AiCopilotMessageList } from './ai-copilot-message-list'
import { AiCopilotSetup } from './ai-copilot-setup'

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
    reasoningEffort,
    selectProvider,
    selectModel,
    selectReasoningEffort,
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
    executeAiTerminalHandoff,
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

    const approvalRequestId = activity.proposal.approvalRequestId
    if (!approvalRequestId) {
      showCommandActionMessage(t.aiCopilotTerminalInputWriteFailed)
      return Promise.reject(new Error(t.aiCopilotTerminalInputWriteFailed))
    }
    return executeAiTerminalHandoff(approvalRequestId, targetTabId, activity.proposal.command)
      .then(() => {
        showCommandActionMessage(t.aiCopilotTerminalInputWritten)
      })
      .catch((error) => {
        showCommandActionMessage(t.aiCopilotTerminalInputWriteFailed)
        throw error
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
      <AiCopilotHeader
        canChat={canChat}
        isStreaming={isStreaming}
        onClose={onClose}
        onOpenSettings={onOpenSettings}
        openNewConversation={openNewConversation}
      />

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
          <AiCopilotSetup onOpenSettings={onOpenSettings} />
        ) : isConversationListOpen ? (
          <AiCopilotConversationList
            conversation={conversation}
            conversationSearch={conversationSearch}
            filteredConversations={filteredConversations}
            isStreaming={isStreaming}
            loadConversation={loadConversation}
            onCloseConversationList={() => setIsConversationListOpen(false)}
            onDeleteConversation={(item) => setConversationPendingDeletion(item)}
            openNewConversation={openNewConversation}
            setConversationSearch={setConversationSearch}
          />
        ) : (
          <>
            <AiCopilotChatMeta
              commandActionMessage={commandActionMessage}
              conversation={conversation}
              conversationTitleDraft={conversationTitleDraft}
              deleteCurrentConversation={deleteCurrentConversation}
              isDeleteConfirmationOpen={isDeleteConfirmationOpen}
              isRenamingConversation={isRenamingConversation}
              isStreaming={isStreaming}
              onOpenConversationList={() => setIsConversationListOpen(true)}
              saveConversationTitle={saveConversationTitle}
              setConversationTitleDraft={setConversationTitleDraft}
              setIsDeleteConfirmationOpen={setIsDeleteConfirmationOpen}
              setIsRenamingConversation={setIsRenamingConversation}
            />

            <AiCopilotMessageList
              canRetry={canRetry}
              conversation={conversation}
              errorMessage={errorMessage}
              executeCommandInTerminal={executeCommandInTerminal}
              isStreaming={isStreaming}
              messageViewportRef={messageViewportRef}
              onDeleteMessage={(message) => setMessagePendingDeletion(message)}
              onRetryLastRequest={retryLastRequest}
              resolveToolApproval={resolveToolApproval}
              resolvingToolApprovalIds={resolvingToolApprovalIds}
              toolActivities={toolActivities}
              toolApprovalRequests={toolApprovalRequests}
            />
            {usage ? (
              <div className="ai-copilot-usage" aria-label={t.aiCopilotUsage}>
                {t.aiCopilotUsage}: {usage.inputTokens ?? '—'} / {usage.outputTokens ?? '—'}
              </div>
            ) : null}
          </>
        )}
      </div>

      {!isConversationListOpen ? (
        <AiCopilotComposer
          canChat={canChat}
          composerCompositionRef={composerCompositionRef}
          copilotMode={copilotMode}
          copilotModeDescription={copilotModeDescription}
          copilotModeIconName={copilotModeIconName}
          currentProvider={currentProvider}
          draft={draft}
          isContextPreviewing={isContextPreviewing}
          isStreaming={isStreaming}
          isTerminalTarget={isTerminalTarget}
          onComposerKeyDown={onComposerKeyDown}
          providers={providers}
          referenceTerminal={referenceTerminal}
          reasoningEffort={reasoningEffort}
          requiresTerminalContext={requiresTerminalContext}
          selectCopilotMode={selectCopilotMode}
          selectModel={selectModel}
          selectProvider={selectProvider}
          selectReasoningEffort={selectReasoningEffort}
          selectedModel={selectedModel}
          selectedProviderId={selectedProviderId}
          send={send}
          setDraft={setDraft}
          stop={stop}
          toggleTerminalReference={toggleTerminalReference}
        />
      ) : null}
      {!isConversationListOpen ? (
        <AiCopilotDangerousDock
          dangerousCommandRestrictionsEnabled={dangerousCommandRestrictionsEnabled}
          isStreaming={isStreaming}
          modeState={modeState}
          showsDangerousCommandRestrictions={showsDangerousCommandRestrictions}
          toggleDangerousCommandRestrictions={toggleDangerousCommandRestrictions}
        />
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
