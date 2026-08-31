import { useEffect, useRef } from 'react'
import { clearTerminalGlobalState, createTerminalLifecycleRuntime } from './terminal-lifecycle-core'
import { registerTerminalInputHandlers } from './terminal-lifecycle-input'
import { registerTerminalInteractionHandlers } from './terminal-lifecycle-interactions'
import { registerTerminalOutputHandlers } from './terminal-lifecycle-output'
import type { TerminalLifecycleOptions } from './terminal-lifecycle-types'

export function useTerminalLifecycle(options: TerminalLifecycleOptions) {
  const { isMac, isWindowsPty, isActive, terminalRef } = options
  const optionsRef = useRef(options)
  optionsRef.current = options

  useEffect(() => {
    const runtime = createTerminalLifecycleRuntime(optionsRef.current)
    if (!runtime) {
      return
    }

    const outputHandlers = registerTerminalOutputHandlers(runtime)
    const inputHandlers = registerTerminalInputHandlers(runtime)

    // Register onData before replaying the initial snapshot. A Windows
    // ConPTY shell can put its cursor-position query in that first transcript
    // before the renderer mounts; replaying it earlier would make xterm emit
    // the response before the PTY input boundary was listening.
    runtime.bootTextRef.current = runtime.bootText
    if (runtime.bootText) {
      runtime.replaceTerminalWithTranscript(runtime.terminal, runtime.bootText)
    }

    const sessionEventsCleanup = outputHandlers.registerSessionEvents()
    const interactionHandlers = registerTerminalInteractionHandlers(runtime, outputHandlers, inputHandlers)

    // Ask the main process for the actual PTY size once the terminal is mounted.
    if (!runtime.bootedTabs.current.has(runtime.tabIdRef.current)) {
      runtime.bootedTabs.current.add(runtime.tabIdRef.current)
      outputHandlers.resize()
    }

    return () => {
      interactionHandlers.dispose()
      sessionEventsCleanup()
      inputHandlers.dispose()
      outputHandlers.dispose()

      if (runtime.writeFrameRef.current !== null) {
        window.cancelAnimationFrame(runtime.writeFrameRef.current)
      }
      if (runtime.resizeTimerRef.current !== null) {
        window.cancelAnimationFrame(runtime.resizeTimerRef.current)
      }
      if (runtime.resizeSettleTimerRef.current !== null) {
        window.clearTimeout(runtime.resizeSettleTimerRef.current)
      }
      runtime.writeFrameRef.current = null
      runtime.resizeTimerRef.current = null
      runtime.resizeSettleTimerRef.current = null
      runtime.pendingResizeForceRef.current = false
      runtime.pendingResizeFreezeColsRef.current = false
      runtime.isWritingRef.current = false
      runtime.pendingWriteRef.current = ''
      runtime.renderedTranscriptRef.current = ''
      runtime.suppressHydratedChunksUntilRef.current = 0
      runtime.replayingTranscriptRef.current = false
      runtime.transcriptReplayGenerationRef.current += 1
      runtime.preserveVisibleBufferRef.current = false
      runtime.lastSyncedSizeRef.current = null
      runtime.lastObservedHostRectRef.current = null
      runtime.isHorizontalResizeActiveRef.current = false
      runtime.lastTerminalOutputAtRef.current = 0
      runtime.awaitingCommandCompletionRef.current = false
      runtime.pendingPromptResizeRef.current = false
      runtime.disposeCore()
    }
  }, [isMac, isWindowsPty])

  // A kept-alive terminal must not retain focus after its workspace is hidden.
  // Otherwise the hidden xterm textarea can continue to receive keyboard input
  // and remain the global shortcut owner while another tab is visible.
  useEffect(() => {
    if (isActive) {
      return
    }

    const terminal = terminalRef.current
    if (!terminal) {
      return
    }

    if (document.activeElement === terminal.textarea) {
      terminal.blur()
    }
    clearTerminalGlobalState(terminal)
  }, [isActive, terminalRef])
}
