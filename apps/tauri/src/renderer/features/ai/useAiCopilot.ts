import { useCallback, useEffect, useRef, useState } from 'react'
import type {
  AiCopilotMode,
  AiCopilotModeState,
  AiChatRequest,
  AiConversation,
  AiConversationSummary,
  AiContextAttachment,
  AiContextPreview,
  AiMessage,
  ActionApprovalRequest,
  AiToolActivity,
  AiProviderSummary,
  AiStreamEvent,
  CreateAiContextPreviewInput
} from '@fileterm/core'

type SendMessageOptions = {
  contextSnapshotId?: string
  contextPreview?: AiContextPreview
  mode?: AiCopilotMode
}

type RetryMessageOptions = {
  contextSnapshotId?: string
  mode?: AiCopilotMode
}

function toMessage(error: unknown) {
  const value = String(error)
  return value
    .replace(/^command error:\s*/i, '')
    .replace(/^AI_[A-Z_]+:\s*/i, '')
    .trim()
}

function isChatProvider(provider: AiProviderSummary) {
  return provider.usable
}

function toSummary(conversation: AiConversation): AiConversationSummary {
  return {
    id: conversation.id,
    title: conversation.title,
    providerId: conversation.providerId,
    createdAt: conversation.createdAt,
    updatedAt: conversation.updatedAt,
    messageCount: conversation.messageCount
  }
}

function sortConversations(conversations: AiConversationSummary[]) {
  return [...conversations].sort(
    (left, right) => Number(right.updatedAt) - Number(left.updatedAt) || left.id.localeCompare(right.id)
  )
}

function replaceConversationSummary(
  conversations: AiConversationSummary[],
  conversation: AiConversation
): AiConversationSummary[] {
  return sortConversations([...conversations.filter((item) => item.id !== conversation.id), toSummary(conversation)])
}

function preserveLocalConversationTitle(current: AiConversation | null, incoming: AiConversation): AiConversation {
  if (!current || current.id !== incoming.id || current.title === incoming.title) return incoming
  return { ...incoming, title: current.title }
}

function titleFromMessage(value: string) {
  const compact = value.trim().replace(/\s+/g, ' ')
  return compact.length > 52 ? compact.slice(0, 52) + '…' : compact
}

export function useAiCopilot() {
  const [providers, setProviders] = useState<AiProviderSummary[]>([])
  const [conversations, setConversations] = useState<AiConversationSummary[]>([])
  const [conversation, setConversation] = useState<AiConversation | null>(null)
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null)
  const [selectedModel, setSelectedModel] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [isStreaming, setIsStreaming] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [usage, setUsage] = useState<{ inputTokens?: number; outputTokens?: number } | null>(null)
  const [toolActivities, setToolActivities] = useState<AiToolActivity[]>([])
  const [toolApprovalRequests, setToolApprovalRequests] = useState<ActionApprovalRequest[]>([])
  const [resolvingToolApprovalIds, setResolvingToolApprovalIds] = useState<Set<string>>(() => new Set())
  const [contextPreview, setContextPreview] = useState<AiContextPreview | null>(null)
  const [isContextPreviewing, setIsContextPreviewing] = useState(false)
  const [modeState, setModeState] = useState<AiCopilotModeState | null>(null)
  const conversationRef = useRef<AiConversation | null>(null)
  const selectedProviderIdRef = useRef<string | null>(null)
  const selectedModelRef = useRef<string | null>(null)
  const activeConversationIdRef = useRef<string | null>(null)
  const activeAssistantMessageIdRef = useRef<string | null>(null)
  const activeRequestIdRef = useRef<string | null>(null)
  const requestCompletedRef = useRef(false)
  const chatInFlightRef = useRef(false)
  const cancelRequestedRef = useRef(false)
  const unmountedRef = useRef(false)
  const modeStateRef = useRef<AiCopilotModeState | null>(null)
  const mountedRef = useRef(true)
  const toolApprovalRequestsRef = useRef<ActionApprovalRequest[]>([])
  const resolvingToolApprovalIdsRef = useRef(new Set<string>())

  const applyConversation = useCallback((next: AiConversation | null) => {
    conversationRef.current = next
    activeConversationIdRef.current = next?.id ?? null
    setConversation(next)
  }, [])

  const selectProvider = useCallback((providerId: string | null) => {
    selectedProviderIdRef.current = providerId
    setSelectedProviderId(providerId)
    // Reset model override when provider changes so default model is used
    selectedModelRef.current = null
    setSelectedModel(null)
    setContextPreview(null)
  }, [])

  const selectModel = useCallback((model: string | null) => {
    selectedModelRef.current = model
    setSelectedModel(model)
  }, [])

  const loadConversation = useCallback(
    async (conversationId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming || chatInFlightRef.current) return
      setErrorMessage(null)
      try {
        const next = await desktopApi.getAiConversation(conversationId)
        if (!mountedRef.current) return
        applyConversation(next)
        const provider = providers.find((item) => item.id === next.providerId)
        if (provider && isChatProvider(provider)) {
          selectProvider(provider.id)
        }
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
      }
    },
    [applyConversation, isStreaming, providers, selectProvider]
  )

  const refresh = useCallback(async () => {
    const desktopApi = window.fileterm
    if (!desktopApi) {
      if (mountedRef.current) {
        setProviders([])
        setConversations([])
        setIsLoading(false)
      }
      return
    }
    try {
      const [nextProviders, nextConversations, nextModeState] = await Promise.all([
        desktopApi.listAiProviders(),
        desktopApi.listAiConversations(),
        desktopApi.getAiCopilotModeState()
      ])
      if (!mountedRef.current) return
      modeStateRef.current = nextModeState
      setModeState(nextModeState)
      const availableProviders = nextProviders.filter(isChatProvider)
      setProviders(availableProviders)
      setConversations(sortConversations(nextConversations))
      const nextProviderId =
        availableProviders.find((provider) => provider.id === selectedProviderIdRef.current)?.id ??
        availableProviders.find((provider) => provider.isDefault)?.id ??
        availableProviders[0]?.id ??
        null
      selectProvider(nextProviderId)

      // Opening the Copilot surface starts with a clean draft. Existing local
      // chats remain available from the conversation list and are only loaded
      // after an explicit user selection.
    } catch (error) {
      if (mountedRef.current) {
        setErrorMessage(toMessage(error))
      }
    } finally {
      if (mountedRef.current) {
        setIsLoading(false)
      }
    }
  }, [selectProvider])

  useEffect(() => {
    mountedRef.current = true
    void refresh()
    const reload = () => void refresh()
    window.addEventListener('fileterm:ai-providers-changed', reload)
    return () => {
      mountedRef.current = false
      window.removeEventListener('fileterm:ai-providers-changed', reload)
    }
  }, [refresh])

  useEffect(() => {
    unmountedRef.current = false
    return () => {
      unmountedRef.current = true
      chatInFlightRef.current = false
      cancelRequestedRef.current = true
      const requestId = activeRequestIdRef.current
      activeRequestIdRef.current = null
      // Closing the Copilot surface must also stop its provider request. The
      // Rust service is the cancellation authority; this never mutates the
      // conversation or sends anything to an interactive terminal.
      if (requestId) {
        void window.fileterm?.cancelAiChat(requestId).catch(() => undefined)
      }
    }
  }, [])

  useEffect(() => {
    toolApprovalRequestsRef.current = toolApprovalRequests
  }, [toolApprovalRequests])

  useEffect(() => {
    const desktopApi = window.fileterm
    if (!desktopApi) return

    const dispose = desktopApi.onActionApprovalRequest((request) => {
      if (!mountedRef.current || request.source !== 'ai-copilot') return
      setToolApprovalRequests((current) => {
        if (current.some((item) => item.requestId === request.requestId)) return current
        const next = [...current, request]
        toolApprovalRequestsRef.current = next
        return next
      })
    })

    return () => {
      dispose()
      for (const request of toolApprovalRequestsRef.current) {
        void desktopApi.resolveActionApproval(request.requestId, false).catch(() => undefined)
      }
      toolApprovalRequestsRef.current = []
    }
  }, [])

  const clearToolApprovalState = useCallback(() => {
    toolApprovalRequestsRef.current = []
    setToolApprovalRequests([])
    resolvingToolApprovalIdsRef.current.clear()
    setResolvingToolApprovalIds(new Set())
  }, [])

  const createConversation = useCallback(
    async (providerId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi) {
        throw new Error('FileTerm desktop API is unavailable')
      }
      const next = await desktopApi.createAiConversation({ providerId })
      if (mountedRef.current) {
        applyConversation(next)
        setConversations((current) => replaceConversationSummary(current, next))
      }
      return next
    },
    [applyConversation]
  )

  const restoreConversation = useCallback(
    async (conversationId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi) return
      try {
        const restored = await desktopApi.getAiConversation(conversationId)
        if (mountedRef.current && activeConversationIdRef.current === conversationId) {
          applyConversation(restored)
          setConversations((current) => replaceConversationSummary(current, restored))
        }
      } catch {
        // The stream error is already shown to the user. Do not overwrite it
        // with a secondary local-history read failure.
      }
    },
    [applyConversation]
  )

  const onStreamEvent = useCallback(
    (conversationId: string, event: AiStreamEvent) => {
      if (!mountedRef.current || activeConversationIdRef.current !== conversationId) return
      if (event.type === 'started') {
        activeAssistantMessageIdRef.current = event.messageId
        setToolActivities([])
        clearToolApprovalState()
        return
      }
      if (event.type === 'assistant-message-started') {
        activeAssistantMessageIdRef.current = event.messageId
        return
      }
      if (event.type === 'text-delta') {
        const assistantMessageId = activeAssistantMessageIdRef.current
        if (!assistantMessageId) return
        setConversation((current) => {
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
          conversationRef.current = next
          return next
        })
        return
      }
      if (event.type === 'usage') {
        setUsage({ inputTokens: event.inputTokens, outputTokens: event.outputTokens })
        return
      }
      if (event.type === 'completed') {
        activeAssistantMessageIdRef.current = null
        activeRequestIdRef.current = null
        chatInFlightRef.current = false
        cancelRequestedRef.current = false
        requestCompletedRef.current = true
        const completedConversation = preserveLocalConversationTitle(conversationRef.current, event.conversation)
        applyConversation(completedConversation)
        setConversations((current) => {
          const existing = current.find((item) => item.id === event.conversation.id)
          const conversation =
            existing && existing.title !== event.conversation.title
              ? { ...event.conversation, title: existing.title }
              : completedConversation
          return replaceConversationSummary(current, conversation)
        })
        setIsStreaming(false)
        setErrorMessage(null)
        setToolActivities([])
        clearToolApprovalState()
        return
      }
      if (event.type === 'tool-call') {
        setToolActivities((current) => {
          const existing = current.find((item) => item.proposal.id === event.proposal.id)
          if (existing) return current
          return [...current, { proposal: event.proposal }]
        })
        const assistantMessageId = activeAssistantMessageIdRef.current
        if (assistantMessageId) {
          setConversation((current) => {
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
              conversationRef.current = next
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
            conversationRef.current = next
            return next
          })
        }
        return
      }
      if (event.type === 'tool-result') {
        setToolActivities((current) =>
          current.map((item) =>
            item.proposal.id === event.result.proposalId ? { ...item, result: event.result } : item
          )
        )
        setConversation((current) => {
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
          conversationRef.current = next
          return next
        })
        void window.fileterm
          ?.getAiCopilotModeState()
          .then((nextModeState) => {
            if (!mountedRef.current || activeConversationIdRef.current !== conversationId) return
            modeStateRef.current = nextModeState
            setModeState(nextModeState)
          })
          .catch(() => undefined)
        return
      }
      activeAssistantMessageIdRef.current = null
      activeRequestIdRef.current = null
      chatInFlightRef.current = false
      cancelRequestedRef.current = false
      requestCompletedRef.current = true
      setIsStreaming(false)
      clearToolApprovalState()
      // A user stop (or a surface teardown) is a successful cancellation
      // path, not a retryable Provider error. Restore the persisted local
      // conversation so any partial assistant delta disappears, while
      // keeping the submitted user message available in history.
      setErrorMessage(event.code === 'AI_REQUEST_CANCELLED' ? null : event.message)
      void restoreConversation(conversationId)
    },
    [applyConversation, clearToolApprovalState, restoreConversation]
  )

  const startRequest = useCallback(
    async (
      request: (
        conversationId: string,
        providerId: string,
        onEvent: (event: AiStreamEvent) => void
      ) => Promise<AiChatRequest>,
      conversationId: string,
      providerId: string
    ) => {
      requestCompletedRef.current = false
      const result = await request(conversationId, providerId, (event) => onStreamEvent(conversationId, event))
      if (!mountedRef.current || unmountedRef.current) {
        void window.fileterm?.cancelAiChat(result.requestId).catch(() => undefined)
      } else if (cancelRequestedRef.current && !requestCompletedRef.current) {
        void window.fileterm?.cancelAiChat(result.requestId).catch(() => undefined)
      } else if (!requestCompletedRef.current) {
        activeRequestIdRef.current = result.requestId
      }
      return result
    },
    [onStreamEvent]
  )

  const autoSummarizeConversationTitle = useCallback(
    async (conversationId: string, providerId: string, modelOverride?: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi) return

      try {
        const renamed = await desktopApi.summarizeAiConversationTitle({
          conversationId,
          providerId,
          modelOverride
        })
        if (!mountedRef.current) return

        // The chat stream may still be producing assistant text. Merge only
        // the generated title so a title request cannot replace the visible
        // in-flight message with its shorter persisted snapshot.
        setConversation((current) => {
          if (!current || current.id !== renamed.id) return current
          const next = { ...current, title: renamed.title, updatedAt: renamed.updatedAt }
          conversationRef.current = next
          return next
        })
        setConversations((current) => {
          const existing = current.some((item) => item.id === renamed.id)
          if (!existing) return replaceConversationSummary(current, renamed)
          return sortConversations(
            current.map((item) =>
              item.id === renamed.id ? { ...item, title: renamed.title, updatedAt: renamed.updatedAt } : item
            )
          )
        })
      } catch {
        // OpenCode treats automatic title generation as best effort. A title
        // failure must never surface as a chat failure or interrupt streaming.
      }
    },
    []
  )

  const setDangerousCommandRestrictions = useCallback(async (enabled: boolean) => {
    const desktopApi = window.fileterm
    if (!desktopApi) return null
    try {
      const next = await desktopApi.setAiDangerousCommandRestrictions({ enabled })
      if (mountedRef.current) {
        modeStateRef.current = next
        setModeState(next)
      }
      return next
    } catch (error) {
      if (mountedRef.current) setErrorMessage(toMessage(error))
      return null
    }
  }, [])

  const sendMessage = useCallback(
    async (value: string, options: SendMessageOptions = {}) => {
      const desktopApi = window.fileterm
      const content = value.trim()
      const providerId = selectedProviderIdRef.current
      if (!desktopApi || !content || !providerId || isStreaming || chatInFlightRef.current) return false
      chatInFlightRef.current = true
      cancelRequestedRef.current = false
      const mode = options.mode ?? modeStateRef.current?.mode ?? 'pure-conversation'
      const preview =
        options.contextSnapshotId && options.contextPreview?.snapshotId === options.contextSnapshotId
          ? options.contextPreview
          : options.contextSnapshotId && contextPreview?.snapshotId === options.contextSnapshotId
            ? contextPreview
            : null
      const context: AiContextAttachment | undefined = preview
        ? {
            mode: preview.mode,
            target: preview.target,
            redactions: preview.redactions,
            truncated: preview.truncated
          }
        : undefined
      setErrorMessage(null)
      setUsage(null)
      setToolActivities([])
      clearToolApprovalState()
      setIsStreaming(true)

      let target = conversationRef.current
      try {
        if (!target) {
          target = await createConversation(providerId)
        }
        const temporaryMessageId = `pending-ai-message-${Date.now()}`
        const timestamp = String(Date.now())
        const optimisticMessage: AiMessage = {
          id: temporaryMessageId,
          role: 'user',
          content,
          createdAt: timestamp,
          context
        }
        const shouldSummarizeTitle = target.messages.length === 0
        const optimisticConversation: AiConversation = {
          ...target,
          title: target.messages.length === 0 ? titleFromMessage(content) : target.title,
          providerId,
          updatedAt: timestamp,
          messageCount: target.messages.length + 1,
          messages: [...target.messages, optimisticMessage]
        }
        applyConversation(optimisticConversation)
        const modelOverride = selectedModelRef.current || undefined
        const result = await startRequest(
          (conversationId, requestProviderId, onEvent) =>
            desktopApi.startAiChat(
              {
                conversationId,
                providerId: requestProviderId,
                modelOverride,
                userMessage: content,
                contextSnapshotId: options.contextSnapshotId,
                mode
              },
              onEvent
            ),
          target.id,
          providerId
        )
        if (mountedRef.current) {
          setConversation((current) => {
            if (!current || current.id !== target?.id) return current
            const messages = current.messages.map((message) =>
              message.id === temporaryMessageId ? { ...message, id: result.userMessageId } : message
            )
            const next = { ...current, messages }
            conversationRef.current = next
            return next
          })
        }
        if (options.contextSnapshotId && mountedRef.current) {
          setContextPreview((current) => (current?.snapshotId === options.contextSnapshotId ? null : current))
        }
        if (shouldSummarizeTitle) {
          // The Rust start command persists the first user message before it
          // returns, so the title request sees the same local history that
          // OpenCode uses for its one-shot title generation.
          void autoSummarizeConversationTitle(target.id, providerId, modelOverride)
        }
        return true
      } catch (error) {
        if (mountedRef.current) {
          setConversation((current) => {
            if (!current || current.id !== target?.id) return current
            const messages = current.messages.filter((message) => !message.id.startsWith('pending-ai-message-'))
            const next = { ...current, messages, messageCount: messages.length }
            conversationRef.current = next
            return next
          })
          setErrorMessage(toMessage(error))
          setIsStreaming(false)
          activeRequestIdRef.current = null
          chatInFlightRef.current = false
          cancelRequestedRef.current = false
          requestCompletedRef.current = true
          if (options.contextSnapshotId) {
            setContextPreview((current) => (current?.snapshotId === options.contextSnapshotId ? null : current))
          }
        }
        return false
      }
    },
    [
      applyConversation,
      autoSummarizeConversationTitle,
      clearToolApprovalState,
      contextPreview,
      createConversation,
      isStreaming,
      startRequest
    ]
  )

  const createContextPreview = useCallback(
    async (input: CreateAiContextPreviewInput) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return null
      setErrorMessage(null)
      setIsContextPreviewing(true)
      try {
        const preview = await desktopApi.createAiContextPreview(input)
        if (mountedRef.current) {
          setContextPreview(preview)
        }
        return preview
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
        return null
      } finally {
        if (mountedRef.current) {
          setIsContextPreviewing(false)
        }
      }
    },
    [isStreaming]
  )

  const clearContextPreview = useCallback(() => {
    if (!isStreaming) {
      setContextPreview(null)
    }
  }, [isStreaming])

  const retry = useCallback(
    async (options: RetryMessageOptions = {}) => {
      const desktopApi = window.fileterm
      const currentConversation = conversationRef.current
      const providerId = selectedProviderIdRef.current
      if (!desktopApi || !currentConversation || !providerId || isStreaming || chatInFlightRef.current) return false
      chatInFlightRef.current = true
      cancelRequestedRef.current = false
      const mode = options.mode ?? modeStateRef.current?.mode ?? 'pure-conversation'
      setErrorMessage(null)
      setUsage(null)
      setToolActivities([])
      clearToolApprovalState()
      setContextPreview(null)
      setIsStreaming(true)
      try {
        const modelOverride = selectedModelRef.current || undefined
        await startRequest(
          (conversationId, requestProviderId, onEvent) =>
            desktopApi.retryAiChat(
              {
                conversationId,
                providerId: requestProviderId,
                modelOverride,
                contextSnapshotId: options.contextSnapshotId,
                mode
              },
              onEvent
            ),
          currentConversation.id,
          providerId
        )
        return true
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
          setIsStreaming(false)
          activeRequestIdRef.current = null
          chatInFlightRef.current = false
          cancelRequestedRef.current = false
          requestCompletedRef.current = true
        }
        return false
      }
    },
    [clearToolApprovalState, isStreaming, startRequest]
  )

  const setCopilotMode = useCallback(
    async (mode: AiCopilotMode, confirmed = false) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return null
      setErrorMessage(null)
      try {
        const next = await desktopApi.setAiCopilotMode({ mode, confirmed })
        if (mountedRef.current) {
          modeStateRef.current = next
          setModeState(next)
          setContextPreview(null)
        }
        return next
      } catch (error) {
        if (mountedRef.current) setErrorMessage(toMessage(error))
        return null
      }
    },
    [isStreaming]
  )

  const setContextAttach = useCallback(
    async (attachTerminalContext: boolean) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return null
      setErrorMessage(null)
      try {
        const next = await desktopApi.setAiContextAttach({ attachTerminalContext })
        if (mountedRef.current) {
          modeStateRef.current = next
          setModeState(next)
          setContextPreview(null)
        }
        return next
      } catch (error) {
        if (mountedRef.current) setErrorMessage(toMessage(error))
        return null
      }
    },
    [isStreaming]
  )

  const stop = useCallback(async () => {
    const desktopApi = window.fileterm
    const requestId = activeRequestIdRef.current
    if (!desktopApi || (!requestId && !chatInFlightRef.current)) return
    cancelRequestedRef.current = true
    if (!requestId) return
    try {
      await desktopApi.cancelAiChat(requestId)
    } catch (error) {
      if (mountedRef.current) {
        setErrorMessage(toMessage(error))
      }
    }
  }, [])

  const resolveToolApproval = useCallback(async (requestId: string, approved: boolean, riskAcknowledged = false) => {
    const desktopApi = window.fileterm
    const request = toolApprovalRequestsRef.current.find((item) => item.requestId === requestId)
    if (!desktopApi || !request || resolvingToolApprovalIdsRef.current.has(requestId)) return
    if (approved && request.requiresRiskAcknowledgement && !riskAcknowledged) return

    resolvingToolApprovalIdsRef.current.add(requestId)
    setResolvingToolApprovalIds(new Set(resolvingToolApprovalIdsRef.current))
    try {
      await desktopApi.resolveActionApproval(requestId, approved)
    } catch (error) {
      resolvingToolApprovalIdsRef.current.delete(requestId)
      setResolvingToolApprovalIds(new Set(resolvingToolApprovalIdsRef.current))
      if (mountedRef.current) setErrorMessage(toMessage(error))
    }
  }, [])

  const executeAiTerminalHandoff = useCallback(async (requestId: string, tabId: string, command: string) => {
    const desktopApi = window.fileterm
    const request = toolApprovalRequestsRef.current.find((item) => item.requestId === requestId)
    if (!desktopApi) throw new Error('FileTerm desktop bridge is unavailable')
    if (!request) throw new Error('Copilot approval is no longer pending')
    if (resolvingToolApprovalIdsRef.current.has(requestId)) {
      throw new Error('Copilot approval is already being handled')
    }

    resolvingToolApprovalIdsRef.current.add(requestId)
    setResolvingToolApprovalIds(new Set(resolvingToolApprovalIdsRef.current))
    try {
      await desktopApi.executeAiTerminalHandoff(requestId, tabId, command)
    } catch (error) {
      resolvingToolApprovalIdsRef.current.delete(requestId)
      setResolvingToolApprovalIds(new Set(resolvingToolApprovalIdsRef.current))
      if (mountedRef.current) setErrorMessage(toMessage(error))
      throw error
    }
  }, [])

  const renameConversation = useCallback(
    async (conversationId: string, title: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return false
      setErrorMessage(null)
      try {
        const renamed = await desktopApi.renameAiConversation({ conversationId, title })
        if (mountedRef.current) {
          if (conversationRef.current?.id === renamed.id) {
            applyConversation(renamed)
          }
          setConversations((current) => replaceConversationSummary(current, renamed))
        }
        return true
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
        return false
      }
    },
    [applyConversation, isStreaming]
  )

  const deleteConversation = useCallback(
    async (conversationId: string) => {
      const desktopApi = window.fileterm
      // Keep the active stream's conversation intact, but allow housekeeping
      // of an idle history item while another conversation is generating.
      const isDeletingActiveConversation = conversationRef.current?.id === conversationId
      if (!desktopApi || (chatInFlightRef.current && isDeletingActiveConversation)) return false
      setErrorMessage(null)
      try {
        await desktopApi.deleteAiConversation(conversationId)
        if (mountedRef.current) {
          const wasActive = conversationRef.current?.id === conversationId
          setConversations((current) => current.filter((item) => item.id !== conversationId))
          if (wasActive) {
            activeAssistantMessageIdRef.current = null
            setUsage(null)
            setContextPreview(null)
            applyConversation(null)
          }
        }
        return true
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
        return false
      }
    },
    [applyConversation, isStreaming]
  )

  const deleteMessage = useCallback(
    async (conversationId: string, messageId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi || chatInFlightRef.current || conversationRef.current?.id !== conversationId) return false
      setErrorMessage(null)
      try {
        const updated = await desktopApi.deleteAiMessage({ conversationId, messageId })
        if (mountedRef.current) {
          const next = preserveLocalConversationTitle(conversationRef.current, updated)
          applyConversation(next)
          setConversations((current) => replaceConversationSummary(current, next))
        }
        return true
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
        return false
      }
    },
    [applyConversation, isStreaming]
  )

  const newChat = useCallback(() => {
    if (isStreaming || chatInFlightRef.current) return
    activeAssistantMessageIdRef.current = null
    setErrorMessage(null)
    setUsage(null)
    setToolActivities([])
    setContextPreview(null)
    clearToolApprovalState()
    applyConversation(null)
  }, [applyConversation, clearToolApprovalState, isStreaming])

  const currentProvider = providers.find((provider) => provider.id === selectedProviderId) ?? null
  // Effective model: user-selected override > provider's default model
  const effectiveModel = selectedModel || currentProvider?.model || null

  return {
    providers,
    conversations,
    conversation,
    currentProvider,
    selectedProviderId,
    selectedModel,
    effectiveModel,
    isLoading,
    isStreaming,
    errorMessage,
    usage,
    toolActivities,
    toolApprovalRequests,
    resolvingToolApprovalIds,
    contextPreview,
    isContextPreviewing,
    modeState,
    selectProvider,
    selectModel,
    loadConversation,
    refresh,
    newChat,
    renameConversation,
    deleteMessage,
    deleteConversation,
    createContextPreview,
    clearContextPreview,
    sendMessage,
    setCopilotMode,
    setContextAttach,
    setDangerousCommandRestrictions,
    resolveToolApproval,
    executeAiTerminalHandoff,
    retry,
    stop
  }
}
