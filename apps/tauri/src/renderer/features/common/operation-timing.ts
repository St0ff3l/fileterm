const DEFAULT_MINIMUM_BUSY_DURATION_MS = 360

export function waitForMinimumBusyDuration(
  startedAt: number,
  minimumDurationMs = DEFAULT_MINIMUM_BUSY_DURATION_MS
): Promise<void> {
  const elapsed = performance.now() - startedAt
  const remaining = Math.max(0, minimumDurationMs - elapsed)

  if (remaining === 0) {
    return Promise.resolve()
  }

  return new Promise((resolve) => {
    window.setTimeout(resolve, remaining)
  })
}
