import type { Dispatch, SetStateAction } from 'react'
import type {
  ActionApprovalRequest,
  AiConversation,
  AiConversationSummary,
  AiCopilotModeState,
  AiMessage,
  AiStreamEvent,
  AiToolActivity
} from '@fileterm/core'
import { preserveLocalConversationTitle, replaceConversationSummary } from './ai-copilot-utils'

type MutableRef<T> = { current: T }

export type AiCopilotStreamState = {
  mountedRef: MutableRef<boolean>
  activeConversationIdRef: MutableRef<string | null>
  activeAssistantMessageIdRef: MutableRef<string | null>
  activeRequestIdRef: MutableRef<string | null>
  requestCompletedRef: MutableRef<boolean>
  chatInFlightRef: MutableRef<boolean>
  cancelRequestedRef: MutableRef<boolean>
  conversationRef: MutableRef<AiConversation | null>
  modeStateRef: MutableRef<AiCopilotModeState | null>
  toolApprovalRequestsRef: MutableRef<ActionApprovalRequest[]>
  setConversation: Dispatch<SetStateAction<AiConversation | null>>
  setConversations: Dispatch<SetStateAction<AiConversationSummary[]>>
  setUsage: Dispatch<SetStateAction<{ inputTokens?: number; outputTokens?: number } | null>>
  setToolActivities: Dispatch<SetStateAction<AiToolActivity[]>>
  setModeState: Dispatch<SetStateAction<AiCopilotModeState | null>>
  setIsStreaming: Dispatch<SetStateAction<boolean>>
  setErrorMessage: Dispatch<SetStateAction<string | null>>
  clearToolApprovalState(): void
  applyConversation(next: AiConversation | null): void
  restoreConversation(conversationId: string): void | Promise<void>
}

export function handleAiStreamEvent(state: AiCopilotStreamState, conversationId: string, event: AiStreamEvent) {
  if (!state.mountedRef.current || state.activeConversationIdRef.current !== conversationId) return
  if (event.type === 'started') {
    state.activeAssistantMessageIdRef.current = event.messageId
    state.setToolActivities([])
    state.clearToolApprovalState()
    return
  }
  if (event.type === 'assistant-message-started') {
    state.activeAssistantMessageIdRef.current = event.messageId
    return
  }
  if (event.type === 'text-delta') {
    const assistantMessageId = state.activeAssistantMessageIdRef.current
    if (!assistantMessageId) return
    state.setConversation((current) => {
      if (!current || current.id !== conversationId) return current
      const existingMessage = current.messages.find((message) => message.id === assistantMessageId)
      const timestamp = String(Date.now())
      const messages: AiMessage[] = existingMessage
        ? current.messages.map((message) =>
            message.id === assistantMessageId ? { ...message, content: message.content + event.text } : message
          )
        : [
            ...current.messages,
            { id: assistantMessageId, role: 'assistant', content: event.text, createdAt: timestamp }
          ]
      const next = { ...current, messages, messageCount: messages.length, updatedAt: timestamp }
      state.conversationRef.current = next
      return next
    })
    return
  }
  if (event.type === 'usage') {
    state.setUsage({ inputTokens: event.inputTokens, outputTokens: event.outputTokens })
    return
  }
  if (event.type === 'completed') {
    state.activeAssistantMessageIdRef.current = null
    state.activeRequestIdRef.current = null
    state.chatInFlightRef.current = false
    state.cancelRequestedRef.current = false
    state.requestCompletedRef.current = true
    const completedConversation = preserveLocalConversationTitle(state.conversationRef.current, event.conversation)
    state.applyConversation(completedConversation)
    state.setConversations((current) => {
      const existing = current.find((item) => item.id === event.conversation.id)
      const conversation =
        existing && existing.title !== event.conversation.title
          ? { ...event.conversation, title: existing.title }
          : completedConversation
      return replaceConversationSummary(current, conversation)
    })
    state.setIsStreaming(false)
    state.setErrorMessage(null)
    state.setToolActivities([])
    state.clearToolApprovalState()
    return
  }
  if (event.type === 'tool-call') {
    state.setToolActivities((current) => {
      const existing = current.find((item) => item.proposal.id === event.proposal.id)
      if (existing) return current
      return [...current, { proposal: event.proposal }]
    })
    const assistantMessageId = state.activeAssistantMessageIdRef.current
    if (assistantMessageId) {
      state.setConversation((current) => {
        if (!current || current.id !== conversationId) return current
        const timestamp = String(Date.now())
        const messageIndex = current.messages.findIndex((message) => message.id === assistantMessageId)
        if (messageIndex < 0) {
          const next = {
            ...current,
            messages: [
              ...current.messages,
              {
                id: assistantMessageId,
                role: 'assistant' as const,
                content: '',
                createdAt: timestamp,
                toolActivities: [{ proposal: event.proposal }]
              }
            ],
            messageCount: current.messages.length + 1,
            updatedAt: timestamp
          }
          state.conversationRef.current = next
          return next
        }
        const message = current.messages[messageIndex]
        if (message.toolActivities?.some((activity) => activity.proposal.id === event.proposal.id)) return current
        const messages = current.messages.map((item) =>
          item.id === assistantMessageId
            ? { ...item, toolActivities: [...(item.toolActivities ?? []), { proposal: event.proposal }] }
            : item
        )
        const next = { ...current, messages, updatedAt: timestamp }
        state.conversationRef.current = next
        return next
      })
    }
    return
  }
  if (event.type === 'tool-result') {
    state.setToolActivities((current) =>
      current.map((item) => (item.proposal.id === event.result.proposalId ? { ...item, result: event.result } : item))
    )
    state.setConversation((current) => {
      if (!current || current.id !== conversationId) return current
      let changed = false
      const messages = current.messages.map((message) => {
        if (!message.toolActivities?.some((activity) => activity.proposal.id === event.result.proposalId)) {
          return message
        }
        changed = true
        return {
          ...message,
          toolActivities: message.toolActivities.map((activity) =>
            activity.proposal.id === event.result.proposalId ? { ...activity, result: event.result } : activity
          )
        }
      })
      if (!changed) return current
      const next = { ...current, messages, updatedAt: String(Date.now()) }
      state.conversationRef.current = next
      return next
    })
    void window.fileterm
      ?.getAiCopilotModeState()
      .then((nextModeState) => {
        if (!state.mountedRef.current || state.activeConversationIdRef.current !== conversationId) return
        state.modeStateRef.current = nextModeState
        state.setModeState(nextModeState)
      })
      .catch(() => undefined)
    return
  }
  state.activeAssistantMessageIdRef.current = null
  state.activeRequestIdRef.current = null
  state.chatInFlightRef.current = false
  state.cancelRequestedRef.current = false
  state.requestCompletedRef.current = true
  state.setIsStreaming(false)
  state.clearToolApprovalState()
  // A user stop (or a surface teardown) is a successful cancellation path,
  // not a retryable Provider error. Restore the persisted local conversation
  // so any partial assistant delta disappears, while keeping the submitted
  // user message available in history.
  state.setErrorMessage(event.code === 'AI_REQUEST_CANCELLED' ? null : event.message)
  void state.restoreConversation(conversationId)
}
