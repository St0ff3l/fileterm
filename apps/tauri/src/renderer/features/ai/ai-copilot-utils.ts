import type {
  AiConversation,
  AiConversationSummary,
  AiCopilotMode,
  AiContextPreview,
  AiProviderSummary
} from '@fileterm/core'

export type SendMessageOptions = {
  contextSnapshotId?: string
  contextPreview?: AiContextPreview
  mode?: AiCopilotMode
}

export type RetryMessageOptions = {
  contextSnapshotId?: string
  mode?: AiCopilotMode
}

export function toMessage(error: unknown) {
  const value = String(error)
  return value
    .replace(/^command error:\s*/i, '')
    .replace(/^AI_[A-Z_]+:\s*/i, '')
    .trim()
}

export function isChatProvider(provider: AiProviderSummary) {
  return provider.usable
}

export function toSummary(conversation: AiConversation): AiConversationSummary {
  return {
    id: conversation.id,
    title: conversation.title,
    providerId: conversation.providerId,
    createdAt: conversation.createdAt,
    updatedAt: conversation.updatedAt,
    messageCount: conversation.messageCount
  }
}

export function sortConversations(conversations: AiConversationSummary[]) {
  return [...conversations].sort(
    (left, right) => Number(right.updatedAt) - Number(left.updatedAt) || left.id.localeCompare(right.id)
  )
}

export function replaceConversationSummary(
  conversations: AiConversationSummary[],
  conversation: AiConversation
): AiConversationSummary[] {
  return sortConversations([...conversations.filter((item) => item.id !== conversation.id), toSummary(conversation)])
}

export function preserveLocalConversationTitle(
  current: AiConversation | null,
  incoming: AiConversation
): AiConversation {
  if (!current || current.id !== incoming.id || current.title === incoming.title) return incoming
  return { ...incoming, title: current.title }
}

export function titleFromMessage(value: string) {
  const compact = value.trim().replace(/\s+/g, ' ')
  return compact.length > 52 ? compact.slice(0, 52) + '…' : compact
}
