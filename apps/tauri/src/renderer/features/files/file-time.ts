/**
 * File listing timestamps are sent by the backend as UTC values. Keep the
 * conversion in the renderer so the file pane follows the user's system
 * timezone instead of the remote host or a hard-coded UTC display.
 */
export function parseFileModified(value: string): number | null {
  const normalized = value.trim()
  if (!normalized || normalized === '-') {
    return null
  }

  // Older local listings used `YYYY/MM/DD HH:mm` without a timezone suffix.
  // That string was produced from Unix seconds in UTC, so preserve that
  // meaning while the backend and older cached snapshots are still present.
  const legacyMatch = normalized.match(/^(\d{4})\/(\d{2})\/(\d{2}) (\d{2}):(\d{2})(?::(\d{2}))?$/)
  const parsed = legacyMatch
    ? Date.UTC(
        Number(legacyMatch[1]),
        Number(legacyMatch[2]) - 1,
        Number(legacyMatch[3]),
        Number(legacyMatch[4]),
        Number(legacyMatch[5]),
        Number(legacyMatch[6] ?? 0)
      )
    : Date.parse(normalized)

  return Number.isFinite(parsed) ? parsed : null
}

export function formatFileModified(value: string): string {
  const timestamp = parseFileModified(value)
  if (timestamp === null) {
    return value
  }

  const date = new Date(timestamp)
  // Shift the UTC instant by the offset at that instant, including DST, then
  // use ISO's stable field ordering without the trailing UTC marker.
  const localDate = new Date(timestamp - date.getTimezoneOffset() * 60_000)
  return localDate.toISOString().slice(0, 19)
}
