import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type { TerminalCommandHistoryEntry, WorkspaceTab } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { SessionSendTargetPicker } from '../common/SessionSendTargetPicker'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { summarizeSendTarget } from '../common/session-send-targets'

type DockPanel = 'history' | 'options' | null

type DockPreferences = {
  clearAfterSend: boolean
  rememberSendTarget: boolean
}

const HISTORY_LIMIT = 40
const DEFAULT_PREFERENCES: DockPreferences = {
  clearAfterSend: true,
  rememberSendTarget: false
}

function preferencesStorageKey(profileId: string) {
  return `terminal-dock.preferences:${profileId}`
}

function normalizePreferences(value: Partial<DockPreferences> | null | undefined): DockPreferences {
  return {
    clearAfterSend: value?.clearAfterSend ?? DEFAULT_PREFERENCES.clearAfterSend,
    rememberSendTarget: value?.rememberSendTarget ?? DEFAULT_PREFERENCES.rememberSendTarget
  }
}

function isHiddenPath(path: string) {
  return path.split('/').some((segment) => segment.startsWith('.') && segment.length > 1)
}

function formatHistoryTime(timestamp: number) {
  const date = new Date(timestamp)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function TerminalDock({
  activeTab,
  connected,
  sendScope,
  selectedTabIds,
  sendTargets,
  onSendCommand,
  onSendScopeChange,
  onSelectedTabIdsChange
}: {
  activeTab: WorkspaceTab
  connected: boolean
  sendScope: SendScope
  selectedTabIds: string[]
  sendTargets: SessionSendTarget[]
  onSendCommand(command: string): Promise<void>
  onSendScopeChange(scope: SendScope, rememberSelection: boolean): void
  onSelectedTabIdsChange(tabIds: string[], rememberSelection: boolean): void
}) {
  const [command, setCommand] = useState('')
  const [panel, setPanel] = useState<DockPanel>(null)
  const [history, setHistory] = useState<TerminalCommandHistoryEntry[]>([])
  const [preferences, setPreferences] = useState<DockPreferences>(DEFAULT_PREFERENCES)
  const inputRef = useRef<HTMLTextAreaElement | null>(null)
  const rootRef = useRef<HTMLElement | null>(null)

  const persistHistory = async (entries: TerminalCommandHistoryEntry[]) => {
    if (!window.fileterm?.setTerminalCommandHistory) {
      return
    }
    await window.fileterm.setTerminalCommandHistory(activeTab.profileId, entries)
  }

  useEffect(() => {
    let canceled = false

    async function loadHistory() {
      const desktopApi = window.fileterm
      if (!desktopApi?.getTerminalCommandHistory) {
        return
      }

      const storedHistory = await desktopApi.getTerminalCommandHistory(activeTab.profileId)
      if (!canceled) {
        setHistory(storedHistory)
      }
    }

    void loadHistory()

    return () => {
      canceled = true
    }
  }, [activeTab.profileId])

  useEffect(() => {
    if (!panel) return

    function handleClickOutside(event: MouseEvent) {
      const target = event.target as Node
      const clickedInsideDock = rootRef.current && rootRef.current.contains(target)
      const clickedInsideDropdown = (target as HTMLElement).closest && (target as HTMLElement).closest('.custom-select-dropdown')
      
      if (!clickedInsideDock && !clickedInsideDropdown) {
        setPanel(null)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => {
      document.removeEventListener('mousedown', handleClickOutside)
    }
  }, [panel])

  const handleToggleConnection = async () => {
    if (!window.fileterm) return
    if (connected) {
      await window.fileterm.disconnectTab(activeTab.id)
    } else {
      await window.fileterm.reconnectTab(activeTab.id)
    }
  }

  const [historySearch, setHistorySearch] = useState('')
  const [activeHistoryIndex, setActiveHistoryIndex] = useState(0)
  const [activeTokenIndex, setActiveTokenIndex] = useState(0)
  const historySearchInputRef = useRef<HTMLInputElement | null>(null)

  const filteredHistory = useMemo(() => {
    if (!historySearch.trim()) {
      return history
    }
    const q = historySearch.toLowerCase()
    return history.filter((entry) => entry.command.toLowerCase().includes(q))
  }, [history, historySearch])

  useEffect(() => {
    setActiveHistoryIndex((prev) => {
      if (filteredHistory.length === 0) return 0
      return Math.min(prev, filteredHistory.length - 1)
    })
  }, [filteredHistory.length])

  useEffect(() => {
    setActiveTokenIndex(0)
  }, [activeHistoryIndex])

  useEffect(() => {
    if (panel === 'history') {
      setHistorySearch('')
      setActiveHistoryIndex(0)
      setActiveTokenIndex(0)
      historySearchInputRef.current?.focus()
      document.body.setAttribute('data-history-open', 'true')
    } else {
      document.body.removeAttribute('data-history-open')
    }
    return () => {
      document.body.removeAttribute('data-history-open')
    }
  }, [panel])

  useEffect(() => {
    let lastCtrlPress = 0
    const isEventInTerminalZone = (target: EventTarget | null) => {
      if (!(target instanceof Node)) {
        return false
      }

      const terminalArea = rootRef.current?.parentElement
      return Boolean(terminalArea && terminalArea.contains(target))
    }

    const handleGlobalKeyDown = (event: globalThis.KeyboardEvent) => {
      const eventInTerminalZone = isEventInTerminalZone(event.target)
      if (!eventInTerminalZone && panel !== 'history') {
        return
      }

      if (event.key === 'Control' || event.key === 'Meta') {
        const now = Date.now()
        if (!event.repeat && now - lastCtrlPress < 400) {
          event.preventDefault()
          if (document.activeElement === inputRef.current || document.activeElement === historySearchInputRef.current) {
            inputRef.current?.blur()
            historySearchInputRef.current?.blur()
            window.dispatchEvent(new CustomEvent('fileterm:focus-terminal'))
          } else {
            if (panel === 'history') {
              historySearchInputRef.current?.focus()
            } else {
              inputRef.current?.focus()
            }
          }
        }
        lastCtrlPress = now
        return
      }

      if (event.key === 'Alt' && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.repeat) {
        event.preventDefault()
        setPanel((prev) => {
          const next = prev === 'history' ? null : 'history'
          if (next !== 'history') {
            window.dispatchEvent(new CustomEvent('fileterm:focus-terminal'))
          }
          return next
        })
        return
      }

      if (panel === 'history') {
        if (event.key === 'Escape') {
          event.preventDefault()
          setPanel(null)
          window.dispatchEvent(new CustomEvent('fileterm:focus-terminal'))
          return
        }

        if (event.key === 'ArrowUp') {
          event.preventDefault()
          if (filteredHistory.length > 0) {
            setActiveHistoryIndex((prev) => (prev - 1 + filteredHistory.length) % filteredHistory.length)
          }
          return
        }

        if (event.key === 'ArrowDown') {
          event.preventDefault()
          if (filteredHistory.length > 0) {
            setActiveHistoryIndex((prev) => (prev + 1) % filteredHistory.length)
          }
          return
        }

        if (event.key === 'ArrowLeft') {
          event.preventDefault()
          const targetItem = filteredHistory[activeHistoryIndex]
          if (targetItem) {
            const tokens = targetItem.command.split(/\s+/).filter(Boolean)
            if (tokens.length > 0) {
              setActiveTokenIndex((prev) => (prev - 1 + tokens.length) % tokens.length)
            }
          }
          return
        }

        if (event.key === 'ArrowRight') {
          event.preventDefault()
          const targetItem = filteredHistory[activeHistoryIndex]
          if (targetItem) {
            const tokens = targetItem.command.split(/\s+/).filter(Boolean)
            if (tokens.length > 0) {
              setActiveTokenIndex((prev) => (prev + 1) % tokens.length)
            }
          }
          return
        }

        if (event.key === 'Enter') {
          if (event.isComposing) {
            return
          }
          event.preventDefault()
          const targetItem = filteredHistory[activeHistoryIndex]
          if (targetItem) {
            const tokens = targetItem.command.split(/\s+/).filter(Boolean)
            const selectedToken = tokens[activeTokenIndex]
            if (selectedToken) {
              setCommand((prev) => {
                const trimmed = prev.trim()
                return trimmed ? `${trimmed} ${selectedToken}` : selectedToken
              })
              setPanel(null)
              window.dispatchEvent(new CustomEvent('fileterm:focus-terminal'))
            }
          }
          return
        }
      }
    }
    window.addEventListener('keydown', handleGlobalKeyDown)
    return () => window.removeEventListener('keydown', handleGlobalKeyDown)
  }, [panel, filteredHistory, activeHistoryIndex, activeTokenIndex])

  useEffect(() => {
    let canceled = false

    async function loadPreferences() {
      const raw = await window.fileterm?.getUiStateItem?.(preferencesStorageKey(activeTab.profileId))
      if (!raw || canceled) {
        if (!canceled) {
          setPreferences(DEFAULT_PREFERENCES)
        }
        return
      }

      try {
        const parsed = JSON.parse(raw) as Partial<DockPreferences>
        if (!canceled) {
          setPreferences(normalizePreferences(parsed))
        }
      } catch {
        if (!canceled) {
          setPreferences(DEFAULT_PREFERENCES)
        }
      }
    }

    void loadPreferences()

    return () => {
      canceled = true
    }
  }, [activeTab.profileId])

  const updatePreferences = (updater: (prev: DockPreferences) => DockPreferences) => {
    setPreferences((prev) => {
      const next = updater(prev)
      void window.fileterm?.setUiStateItem?.(preferencesStorageKey(activeTab.profileId), JSON.stringify(next))
      return next
    })
  }

  const canSend = connected && activeTab.sessionType === 'ssh' && command.trim().length > 0
  const activeTargetSummary = summarizeSendTarget(
    sendScope,
    selectedTabIds,
    sendTargets,
    t.commandSendCurrent
  )

  const sendCommand = async (nextCommand: string) => {
    const trimmed = nextCommand.trim()
    if (!trimmed || activeTab.sessionType !== 'ssh' || !connected) {
      return
    }

    await onSendCommand(trimmed)

    const now = Date.now()
    setHistory((prev) => {
      const deduped = prev.filter((entry) => entry.command !== trimmed)
      const next = [{ command: trimmed, createdAt: now }, ...deduped].slice(0, HISTORY_LIMIT)
      void persistHistory(next)
      return next
    })

    if (preferences.clearAfterSend) {
      setCommand('')
    }

    setPanel(null)
    window.requestAnimationFrame(() => inputRef.current?.focus())
  }

  const handleInputKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Escape') {
      if (panel) {
        event.preventDefault()
        setPanel(null)
        return
      }
      if (command) {
        event.preventDefault()
        setCommand('')
      }
      return
    }

    if (event.key === 'Enter' && !event.shiftKey) {
      if (event.nativeEvent.isComposing) {
        return
      }
      event.preventDefault()
      if (canSend) {
        void sendCommand(command)
      }
    }
  }

  const handleHistoryAction = async (cmd: string, actionIndex: number) => {
    if (actionIndex === 0) {
      setPanel(null)
      await sendCommand(cmd)
    } else if (actionIndex === 1) {
      setCommand(cmd)
      setPanel(null)
      window.requestAnimationFrame(() => inputRef.current?.focus())
    } else if (actionIndex === 2) {
      setHistory((prev) => {
        const next = prev.filter((entry) => entry.command !== cmd)
        void persistHistory(next)
        return next
      })
    }
  }

  const activeItemRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (panel === 'history' && activeItemRef.current) {
      activeItemRef.current.scrollIntoView({ block: 'nearest' })
    }
  }, [activeHistoryIndex, panel])

  const renderHistoryPanel = () => (
    <div className="terminal-dock-panel terminal-dock-history">
      <div className="terminal-dock-history-list">
        {filteredHistory.length ? filteredHistory.map((entry, index) => {
          const isActive = activeHistoryIndex === index
          const tokens = entry.command.split(/\s+/).filter(Boolean)
          return (
            <div
              key={`${entry.createdAt}-${entry.command}`}
              ref={isActive ? activeItemRef : undefined}
              className={`terminal-dock-history-wrapper ${isActive ? 'is-active' : ''}`}
              onClick={() => {
                setActiveHistoryIndex(index)
                setActiveTokenIndex(0)
              }}
              onDoubleClick={() => {
                void handleHistoryAction(entry.command, 0)
              }}
            >
              <div className="terminal-dock-history-left">
                <code>
                  {isActive ? (
                    tokens.map((token, tokenIdx) => (
                      <span
                        key={tokenIdx}
                        className={`history-token ${activeTokenIndex === tokenIdx ? 'is-selected' : ''}`}
                        onClick={(e) => {
                          e.stopPropagation()
                          setCommand((prev) => {
                            const trimmed = prev.trim()
                            return trimmed ? `${trimmed} ${token}` : token
                          })
                          setPanel(null)
                          window.dispatchEvent(new CustomEvent('fileterm:focus-terminal'))
                        }}
                      >
                        {token}
                      </span>
                    ))
                  ) : (
                    entry.command
                  )}
                </code>
              </div>
              <div className="terminal-dock-history-right">
                <span className="terminal-dock-history-time">
                  {formatHistoryTime(entry.createdAt)}
                </span>
                <div className="terminal-dock-history-actions">
                  <button
                    className="terminal-dock-action-btn btn-play"
                    type="button"
                    title={t.send}
                    onClick={(e) => {
                      e.stopPropagation()
                      void handleHistoryAction(entry.command, 0)
                    }}
                  >
                    <AppIcon name="play" />
                  </button>
                  <button
                    className="terminal-dock-action-btn btn-copy"
                    type="button"
                    title={t.copy}
                    onClick={(e) => {
                      e.stopPropagation()
                      void handleHistoryAction(entry.command, 1)
                    }}
                  >
                    <AppIcon name="copy" />
                  </button>
                  <button
                    className="terminal-dock-action-btn btn-delete"
                    type="button"
                    title={t.clear}
                    onClick={(e) => {
                      e.stopPropagation()
                      void handleHistoryAction(entry.command, 2)
                    }}
                  >
                    <span className="close-icon">✖</span>
                  </button>
                </div>
              </div>
            </div>
          )
        }) : (
          <div className="terminal-dock-empty">{t.terminalDockHistoryEmpty}</div>
        )}
      </div>
      <div className="terminal-dock-history-footer">
        <span className="terminal-dock-history-hint">{t.terminalDockHistoryInsertHint}</span>
        <button className="terminal-dock-clear-btn" type="button" onClick={() => {
          setHistory([])
          void persistHistory([])
        }}>
          {t.terminalDockClearList}
        </button>
      </div>
      <div className="terminal-dock-history-search-wrapper">
        <input
          ref={historySearchInputRef}
          className="terminal-dock-history-search"
          placeholder={t.terminalDockHistorySearchPlaceholder}
          type="text"
          value={historySearch}
          onChange={(e) => setHistorySearch(e.target.value)}
        />
      </div>
    </div>
  )

  const renderOptionsPanel = () => (
    <div className="terminal-dock-panel terminal-dock-options">
      <label className="terminal-dock-option-row">
        <input
          checked={preferences.clearAfterSend}
          type="checkbox"
          onChange={(event) => updatePreferences((prev) => ({ ...prev, clearAfterSend: event.currentTarget.checked }))}
        />
        <span>{t.terminalDockClearAfterSend}</span>
      </label>
      <SessionSendTargetPicker
        allLabel={t.commandSendAllWithCount.replace('{count}', String(sendTargets.length))}
        currentLabel={t.commandSendCurrentWithIndex.replace('{index}', String(sendTargets.find((target) => target.tabId === activeTab.id)?.index ?? '-'))}
        onRememberSelectionChange={(nextValue) => updatePreferences((prev) => ({ ...prev, rememberSendTarget: nextValue }))}
        onScopeChange={(nextScope) => onSendScopeChange(nextScope, preferences.rememberSendTarget)}
        onSelectedTabIdsChange={(tabIds) => onSelectedTabIdsChange(tabIds, preferences.rememberSendTarget)}
        rememberSelection={preferences.rememberSendTarget}
        scope={sendScope}
        selectedTabIds={selectedTabIds}
        showRememberSelection
        targets={sendTargets}
      />
    </div>
  )

  const isMac = window.fileterm?.platform === 'darwin'
  const placeholderText = isMac ? t.terminalDockPlaceholderMac : t.terminalDockPlaceholderWin

  return (
    <section ref={rootRef} className="terminal-dock">
      {panel === 'history' ? renderHistoryPanel() : null}
      {panel === 'options' ? renderOptionsPanel() : null}
      {null}
      <div className="terminal-dock-bar">
        <label className="terminal-dock-input-shell">
          <textarea
            ref={inputRef}
            disabled={activeTab.sessionType !== 'ssh'}
            placeholder={placeholderText}
            rows={1}
            wrap="off"
            value={command}
            onChange={(event) => setCommand(event.currentTarget.value)}
            onKeyDown={handleInputKeyDown}
          />
        </label>
        <div className="terminal-dock-actions">
          <button
            aria-pressed={panel === 'history'}
            className={panel === 'history' ? 'is-active' : undefined}
            type="button"
            onClick={() => setPanel((prev) => prev === 'history' ? null : 'history')}
          >
            {t.history}
          </button>
          <button
            aria-pressed={panel === 'options'}
            className={panel === 'options' ? 'is-active' : undefined}
            type="button"
            onClick={() => setPanel((prev) => prev === 'options' ? null : 'options')}
          >
            {`${t.options} · ${activeTargetSummary}`}
          </button>
          <button
            className={`terminal-dock-icon-btn terminal-dock-connection ${connected ? 'is-connected' : 'is-disconnected'}`}
            type="button"
            title={connected ? t.terminalDockDisconnect : t.terminalDockReconnect}
            onClick={handleToggleConnection}
          >
            <AppIcon name="flash" />
          </button>
          <button
            className="terminal-dock-icon-btn"
            type="button"
            title={t.copy}
            onClick={() => window.dispatchEvent(new CustomEvent('fileterm:terminal-copy'))}
          >
            <AppIcon name="copy" />
          </button>
          <button
            className="terminal-dock-icon-btn"
            type="button"
            title={t.paste}
            onClick={() => window.dispatchEvent(new CustomEvent('fileterm:terminal-paste'))}
          >
            <AppIcon name="paste" />
          </button>
          <button
            className="terminal-dock-icon-btn"
            type="button"
            title={t.find}
            onClick={() => window.dispatchEvent(new CustomEvent('fileterm:terminal-find'))}
          >
            <AppIcon name="search" />
          </button>
        </div>
      </div>
    </section>
  )
}
