import { useEffect, useRef } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'

export function TerminalView({
  tabId,
  initialText,
  onStatus
}: {
  tabId: string
  initialText: string
  onStatus?(message: string | null): void
}) {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const initialTextRef = useRef(initialText)
  const bootedTabs = useRef(new Set<string>())
  const wasConnectedRef = useRef(false)

  useEffect(() => {
    if (!hostRef.current) {
      return
    }

    const styles = getComputedStyle(document.documentElement)
    const readColor = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback
    const terminal = new Terminal({
      fontFamily: '"SF Mono", Menlo, Consolas, monospace',
      fontSize: 14,
      lineHeight: 1.45,
      cursorBlink: true,
      allowTransparency: true,
      theme: {
        background: readColor('--terminal-bg', '#1e1e1e'),
        foreground: readColor('--terminal-text', '#e0e0e0'),
        cursor: readColor('--terminal-text', '#e0e0e0'),
        green: readColor('--success', '#39d98a'),
        brightGreen: readColor('--success', '#52f2a0'),
        blue: readColor('--accent-text', '#c8d0da'),
        brightBlue: readColor('--text-main', '#f1f5f9'),
        selectionBackground: readColor('--terminal-cmd-bg', 'rgba(148, 163, 184, 0.24)')
      }
    })
    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.open(hostRef.current)
    fitAddon.fit()

    if (initialTextRef.current) {
      terminal.write(initialTextRef.current)
    }

    const resize = () => {
      fitAddon.fit()
      void window.termdock?.resizeTerminal(tabId, terminal.cols, terminal.rows)
    }

    const onDataDispose = terminal.onData((data) => {
      void window.termdock?.writeTerminal(tabId, data)
    })

    const onResizeDispose = terminal.onResize(({ cols, rows }) => {
      void window.termdock?.resizeTerminal(tabId, cols, rows)
    })

    const offData = window.termdock?.onTerminalData(({ tabId: nextTabId, chunk }) => {
      if (nextTabId === tabId) {
        terminal.write(chunk)
      }
    })

    const offState = window.termdock?.onTerminalState(({ tabId: nextTabId, summary, connected }) => {
      if (nextTabId === tabId) {
        onStatus?.(summary)
        if (wasConnectedRef.current && !connected) {
          terminal.writeln('\r\n[connection closed]')
        }
        wasConnectedRef.current = connected
      }
    })

    const resizeObserver = new ResizeObserver(() => resize())
    resizeObserver.observe(hostRef.current)

    // Ask the main process for the actual PTY size once the terminal is mounted.
    if (!bootedTabs.current.has(tabId)) {
      bootedTabs.current.add(tabId)
      resize()
    }

    return () => {
      onDataDispose.dispose()
      onResizeDispose.dispose()
      offData?.()
      offState?.()
      resizeObserver.disconnect()
      terminal.dispose()
    }
  }, [onStatus, tabId])

  return <div className="terminal-host" ref={hostRef} />
}
