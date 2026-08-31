import type { TerminalCommandHistoryEntry } from '@fileterm/core'

export const TEMPORARY_EDITOR_ID = '__temporary-command-editor__'
export const TEMPORARY_HISTORY_LIMIT = 40

export type TemporaryHistoryEntry = TerminalCommandHistoryEntry & {
  appendCarriageReturn: boolean
}

export function temporaryHistoryKey(entry: TemporaryHistoryEntry) {
  return `${entry.createdAt}-${entry.command}`
}

export function formatTemporaryHistoryTime(createdAt: number) {
  const date = new Date(createdAt)
  const today = new Date()
  const isToday =
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate()

  if (isToday) {
    return [date.getHours(), date.getMinutes()].map((value) => String(value).padStart(2, '0')).join(':')
  }

  return [date.getFullYear(), date.getMonth() + 1, date.getDate()]
    .map((value, index) => (index === 0 ? String(value) : String(value).padStart(2, '0')))
    .join('/')
}
