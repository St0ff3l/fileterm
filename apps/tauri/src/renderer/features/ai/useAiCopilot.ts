import { useCallback, useEffect, useRef, useState } from 'react'
import type {
  AiChatResponseMode,
  AiChatRequest,
  AiConversation,
  AiConversationSummary,
  AiContextAttachment,
  AiContextPreview,
  AiMessage,
  AiProviderSummary,
  AiStreamEvent,
  CreateAiContextPreviewInput
} from '@fileterm/core'

type SendMessageOptions = {
  contextSnapshotId?: string
  responseMode?: AiChatResponseMode
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

function titleFromMessage(value: string) {
  const compact = value.trim().replace(/\s+/g, ' ')
  return compact.length > 52 ? compact.slice(0, 52) + '…' : compact
}

export function useAiCopilot() {
  const [providers, setProviders] = useState<AiProviderSummary[]>([])
  const [conversations, setConversations] = useState<AiConversationSummary[]>([])
  const [conversation, setConversation] = useState<AiConversation | null>(null)
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [isStreaming, setIsStreaming] = useState(false)
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [usage, setUsage] = useState<{ inputTokens?: number; outputTokens?: number } | null>(null)
  const [contextPreview, setContextPreview] = useState<AiContextPreview | null>(null)
  const [isContextPreviewing, setIsContextPreviewing] = useState(false)
  const conversationRef = useRef<AiConversation | null>(null)
  const selectedProviderIdRef = useRef<string | null>(null)
  const activeConversationIdRef = useRef<string | null>(null)
  const activeAssistantMessageIdRef = useRef<string | null>(null)
  const activeResponseModeRef = useRef<AiChatResponseMode>('chat')
  const mountedRef = useRef(true)

  const applyConversation = useCallback((next: AiConversation | null) => {
    conversationRef.current = next
    activeConversationIdRef.current = next?.id ?? null
    setConversation(next)
  }, [])

  const selectProvider = useCallback((providerId: string | null) => {
    selectedProviderIdRef.current = providerId
    setSelectedProviderId(providerId)
    setContextPreview(null)
  }, [])

  const loadConversation = useCallback(
    async (conversationId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return
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
      const [nextProviders, nextConversations] = await Promise.all([
        desktopApi.listAiProviders(),
        desktopApi.listAiConversations()
      ])
      if (!mountedRef.current) return
      const availableProviders = nextProviders.filter(isChatProvider)
      setProviders(availableProviders)
      setConversations(sortConversations(nextConversations))
      const nextProviderId =
        availableProviders.find((provider) => provider.id === selectedProviderIdRef.current)?.id ??
        availableProviders.find((provider) => provider.isDefault)?.id ??
        availableProviders[0]?.id ??
        null
      selectProvider(nextProviderId)

      const currentConversationId = activeConversationIdRef.current
      const initialConversationId =
        currentConversationId ??
        nextConversations.find((item) => item.providerId === nextProviderId)?.id ??
        nextConversations[0]?.id ??
        null
      if (initialConversationId && initialConversationId !== currentConversationId) {
        const initialConversation = await desktopApi.getAiConversation(initialConversationId)
        if (mountedRef.current) {
          applyConversation(initialConversation)
          const conversationProvider = availableProviders.find(
            (provider) => provider.id === initialConversation.providerId
          )
          if (conversationProvider) {
            selectProvider(conversationProvider.id)
          }
        }
      }
    } catch (error) {
      if (mountedRef.current) {
        setErrorMessage(toMessage(error))
      }
    } finally {
      if (mountedRef.current) {
        setIsLoading(false)
      }
    }
  }, [applyConversation, selectProvider])

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
        return
      }
      if (event.type === 'text-delta') {
        if (activeResponseModeRef.current === 'command-proposal') {
          return
        }
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
      if (event.type === 'command') {
        // Command cards are only made visible from the completed, persisted
        // conversation. This event is still useful as a typed transport
        // boundary, but never turns partial model output into a trusted card.
        return
      }
      if (event.type === 'usage') {
        setUsage({ inputTokens: event.inputTokens, outputTokens: event.outputTokens })
        return
      }
      if (event.type === 'completed') {
        activeAssistantMessageIdRef.current = null
        applyConversation(event.conversation)
        setConversations((current) => replaceConversationSummary(current, event.conversation))
        setActiveRequestId(null)
        setIsStreaming(false)
        setErrorMessage(null)
        activeResponseModeRef.current = 'chat'
        return
      }
      activeAssistantMessageIdRef.current = null
      activeResponseModeRef.current = 'chat'
      setActiveRequestId(null)
      setIsStreaming(false)
      setErrorMessage(event.message)
      void restoreConversation(conversationId)
    },
    [applyConversation, restoreConversation]
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
      const result = await request(conversationId, providerId, (event) => onStreamEvent(conversationId, event))
      if (mountedRef.current) {
        setActiveRequestId(result.requestId)
      }
      return result
    },
    [onStreamEvent]
  )

  const sendMessage = useCallback(
    async (value: string, options: SendMessageOptions = {}) => {
      const desktopApi = window.fileterm
      const content = value.trim()
      const providerId = selectedProviderIdRef.current
      if (!desktopApi || !content || !providerId || isStreaming) return false
      const responseMode = options.responseMode ?? 'chat'
      const preview =
        options.contextSnapshotId && contextPreview?.snapshotId === options.contextSnapshotId ? contextPreview : null
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
      setIsStreaming(true)
      activeResponseModeRef.current = responseMode

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
        const optimisticConversation: AiConversation = {
          ...target,
          title: target.messages.length === 0 ? titleFromMessage(content) : target.title,
          providerId,
          updatedAt: timestamp,
          messageCount: target.messages.length + 1,
          messages: [...target.messages, optimisticMessage]
        }
        applyConversation(optimisticConversation)
        const result = await startRequest(
          (conversationId, requestProviderId, onEvent) =>
            desktopApi.startAiChat(
              {
                conversationId,
                providerId: requestProviderId,
                userMessage: content,
                contextSnapshotId: options.contextSnapshotId,
                responseMode
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
          setActiveRequestId(null)
          activeResponseModeRef.current = 'chat'
          if (options.contextSnapshotId) {
            setContextPreview((current) => (current?.snapshotId === options.contextSnapshotId ? null : current))
          }
        }
        return false
      }
    },
    [applyConversation, contextPreview, createConversation, isStreaming, startRequest]
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

  const retry = useCallback(async () => {
    const desktopApi = window.fileterm
    const currentConversation = conversationRef.current
    const providerId = selectedProviderIdRef.current
    if (!desktopApi || !currentConversation || !providerId || isStreaming) return false
    setErrorMessage(null)
    setUsage(null)
    setContextPreview(null)
    setIsStreaming(true)
    try {
      await startRequest(
        (conversationId, requestProviderId, onEvent) =>
          desktopApi.retryAiChat({ conversationId, providerId: requestProviderId }, onEvent),
        currentConversation.id,
        providerId
      )
      return true
    } catch (error) {
      if (mountedRef.current) {
        setErrorMessage(toMessage(error))
        setIsStreaming(false)
        setActiveRequestId(null)
        activeResponseModeRef.current = 'chat'
      }
      return false
    }
  }, [isStreaming, startRequest])

  const stop = useCallback(async () => {
    const desktopApi = window.fileterm
    if (!desktopApi || !activeRequestId) return
    try {
      await desktopApi.cancelAiChat(activeRequestId)
    } catch (error) {
      if (mountedRef.current) {
        setErrorMessage(toMessage(error))
      }
    }
  }, [activeRequestId])

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
      if (!desktopApi || isStreaming) return false
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

  const runReview = useCallback(
    async (commandId: string) => {
      const desktopApi = window.fileterm
      if (!desktopApi || isStreaming) return null
      setErrorMessage(null)
      try {
        const result = await desktopApi.runAiReview({ commandId })
        if (mountedRef.current) {
          if (conversationRef.current?.id === result.conversation.id) {
            applyConversation(result.conversation)
          }
          setConversations((current) => replaceConversationSummary(current, result.conversation))
        }
        return result
      } catch (error) {
        if (mountedRef.current) {
          setErrorMessage(toMessage(error))
        }
        return null
      }
    },
    [applyConversation, isStreaming]
  )

  const newChat = useCallback(() => {
    if (isStreaming) return
    activeAssistantMessageIdRef.current = null
    setErrorMessage(null)
    setUsage(null)
    applyConversation(null)
  }, [applyConversation, isStreaming])

  const currentProvider = providers.find((provider) => provider.id === selectedProviderId) ?? null

  return {
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
    refresh,
    newChat,
    renameConversation,
    deleteConversation,
    createContextPreview,
    clearContextPreview,
    sendMessage,
    runReview,
    retry,
    stop
  }
}
