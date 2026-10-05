export type TerminalInputWriteFailureHandler = (error: unknown) => void

/** Preserve key order across asynchronous Tauri invoke calls for one terminal. */
export function createTerminalInputWriteQueue(options: {
  getTabId(): string
  write(tabId: string, data: string): Promise<void> | undefined
  shouldStop?(): boolean
}) {
  let pending: Promise<void> = Promise.resolve()
  let disposed = false

  const write = (data: string, onFailure?: TerminalInputWriteFailureHandler) => {
    if (disposed) {
      return Promise.resolve()
    }

    // Capture the destination along with the input event so a later render
    // cannot redirect queued characters to a different tab.
    const tabId = options.getTabId()
    const current = pending.then(async () => {
      if (disposed || options.shouldStop?.()) {
        return
      }

      try {
        await options.write(tabId, data)
      } catch (error) {
        try {
          onFailure?.(error)
        } catch {
          // A reporting callback must not leave the input queue rejected.
        }
      }
    })

    // Keep the queue usable if a callback itself fails, while returning the
    // current write promise to callers that need to observe completion.
    pending = current.catch(() => {})
    return current
  }

  return {
    write,
    dispose() {
      disposed = true
    }
  }
}
