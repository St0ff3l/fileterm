export const TERMINAL_OUTPUT_FLUSH_INTERVAL_MS = 16

export type TerminalOutputEmitter = (tabId: string, chunk: string) => void

export class TerminalOutputBatcher {
  private readonly buffers = new Map<string, string[]>()
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly emit: TerminalOutputEmitter
  private disposed = false

  constructor(emit: TerminalOutputEmitter) {
    this.emit = emit
  }

  queue(tabId: string, chunk: string) {
    if (this.disposed) {
      return
    }

    const chunks = this.buffers.get(tabId)
    if (chunks) {
      chunks.push(chunk)
    } else {
      this.buffers.set(tabId, [chunk])
    }

    if (this.timers.has(tabId)) {
      return
    }

    const timer = setTimeout(() => {
      this.timers.delete(tabId)
      this.flush(tabId)
    }, TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)
    this.timers.set(tabId, timer)
  }

  flush(tabId: string) {
    const timer = this.timers.get(tabId)
    if (timer !== undefined) {
      clearTimeout(timer)
      this.timers.delete(tabId)
    }

    const chunks = this.buffers.get(tabId)
    if (!chunks?.length) {
      return
    }
    this.buffers.delete(tabId)

    this.emit(tabId, chunks.length === 1 ? chunks[0]! : chunks.join(''))
  }

  flushAll() {
    for (const tabId of [...this.buffers.keys()]) {
      this.flush(tabId)
    }
  }

  dispose() {
    if (this.disposed) {
      return
    }
    this.disposed = true

    for (const timer of this.timers.values()) {
      clearTimeout(timer)
    }
    this.timers.clear()
    this.buffers.clear()
  }
}
