import { useEffect, useRef, useState } from 'react'
import { AppIcon } from '../common/AppIcon'
import { localizeSerialTerminalText, t } from '../../i18n'

type SerialMacro = { name: string; value: string }

function storageKey(profileId: string) {
  return `fileterm.serial.quick-send.${profileId}`
}

function historyStorageKey(profileId: string) {
  return `fileterm.serial.quick-send-history.${profileId}`
}

function loadMacros(profileId: string): SerialMacro[] {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(storageKey(profileId)) ?? '[]')
    if (!Array.isArray(value)) return []
    return value
      .filter(
        (item): item is SerialMacro =>
          Boolean(item) &&
          typeof item === 'object' &&
          'name' in item &&
          'value' in item &&
          typeof item.name === 'string' &&
          typeof item.value === 'string'
      )
      .slice(-20)
  } catch {
    return []
  }
}

function loadHistory(profileId: string): string[] {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(historyStorageKey(profileId)) ?? '[]')
    if (!Array.isArray(value)) return []
    return value.filter((item): item is string => typeof item === 'string' && item.length > 0).slice(0, 30)
  } catch {
    return []
  }
}

export function SerialQuickSendPanel({
  profileId,
  tabId,
  connected
}: {
  profileId: string
  tabId: string
  connected: boolean
}) {
  const [draft, setDraft] = useState('')
  const [appendNewline, setAppendNewline] = useState(true)
  const [macroName, setMacroName] = useState('')
  const [macros, setMacros] = useState<SerialMacro[]>(() => loadMacros(profileId))
  const [history, setHistory] = useState<string[]>(() => loadHistory(profileId))
  const [loopInterval, setLoopInterval] = useState(1000)
  const [looping, setLooping] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const loopWriteInFlight = useRef(false)

  useEffect(() => {
    setMacros(loadMacros(profileId))
    setHistory(loadHistory(profileId))
    setDraft('')
    setLooping(false)
  }, [profileId])

  useEffect(() => {
    if (!connected) {
      // A reconnect must never silently resume a macro that was running on a
      // different serial session. Require an explicit user action after the
      // port is available again.
      setLooping(false)
      loopWriteInFlight.current = false
    }
  }, [connected])

  useEffect(() => {
    const writeTerminal = window.fileterm?.writeTerminal
    if (!looping || !connected || !draft.trim() || !writeTerminal) return
    const timer = window.setInterval(
      () => {
        if (loopWriteInFlight.current) return
        loopWriteInFlight.current = true
        void writeTerminal(tabId, appendNewline ? `${draft}\r` : draft)
          .catch((error) => {
            setMessage(localizeSerialTerminalText(String(error)))
            setLooping(false)
          })
          .finally(() => {
            loopWriteInFlight.current = false
          })
      },
      Math.min(3_600_000, Math.max(50, loopInterval))
    )
    return () => window.clearInterval(timer)
  }, [appendNewline, connected, draft, loopInterval, looping, tabId])

  const send = async (value: string) => {
    if (!connected || !window.fileterm?.writeTerminal || !value) return
    try {
      await window.fileterm.writeTerminal(tabId, appendNewline ? `${value}\r` : value)
      setHistory((previous) => {
        const next = [value, ...previous.filter((item) => item !== value)].slice(0, 30)
        try {
          window.localStorage.setItem(historyStorageKey(profileId), JSON.stringify(next))
        } catch {
          // History is a convenience; sending must still succeed if storage is unavailable.
        }
        return next
      })
      setMessage(null)
    } catch (error) {
      setMessage(localizeSerialTerminalText(String(error)))
    }
  }

  const saveMacro = () => {
    const name = macroName.trim()
    const value = draft.trim()
    if (!name || !value) {
      setMessage(t.serialQuickSendSaveFailed)
      return
    }
    const next = [...macros.filter((macro) => macro.name !== name), { name, value }].slice(-20)
    setMacros(next)
    setMacroName('')
    setMessage(null)
    try {
      window.localStorage.setItem(storageKey(profileId), JSON.stringify(next))
    } catch {
      setMessage(t.serialQuickSendSaveFailed)
    }
  }

  return (
    <details className="serial-quick-panel">
      <summary>{t.serialQuickSend}</summary>
      <div className="serial-quick-panel__body">
        <div className="serial-quick-panel__compose">
          <input
            disabled={!connected || looping}
            placeholder={t.serialQuickSendPlaceholder}
            spellCheck={false}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                void send(draft)
              }
            }}
          />
          <button disabled={!connected || !draft || looping} type="button" onClick={() => void send(draft)}>
            {t.serialQuickSendNow}
          </button>
          <label className="serial-quick-panel__checkbox">
            <input
              checked={appendNewline}
              disabled={looping}
              type="checkbox"
              onChange={(event) => setAppendNewline(event.target.checked)}
            />
            {t.serialQuickSendNewline}
          </label>
        </div>
        {history.length ? (
          <div className="serial-quick-panel__items">
            <span>{t.serialQuickSendHistory}:</span>
            {history.slice(0, 6).map((item) => (
              <button key={item} type="button" onClick={() => setDraft(item)}>
                {item}
              </button>
            ))}
          </div>
        ) : null}
        <div className="serial-quick-panel__compose">
          <input
            disabled={looping}
            placeholder={t.serialQuickSendMacroName}
            value={macroName}
            onChange={(event) => setMacroName(event.target.value)}
          />
          <button disabled={!draft || !macroName || looping} type="button" onClick={saveMacro}>
            <AppIcon name="plus" size={12} />
            {t.serialQuickSendSaveMacro}
          </button>
        </div>
        {macros.length ? (
          <div className="serial-quick-panel__items">
            <span>{t.serialQuickSendMacros}:</span>
            {macros.map((macro) => (
              <button
                key={macro.name}
                disabled={!connected || looping}
                type="button"
                onClick={() => void send(macro.value)}
              >
                {macro.name}
              </button>
            ))}
          </div>
        ) : null}
        <div className="serial-quick-panel__loop">
          <label>
            {t.serialQuickSendLoopInterval}
            <input
              disabled={looping}
              inputMode="numeric"
              min={50}
              max={3_600_000}
              type="number"
              value={loopInterval}
              onChange={(event) => setLoopInterval(Math.min(3_600_000, Math.max(50, Number(event.target.value) || 50)))}
            />
          </label>
          {looping ? (
            <button type="button" onClick={() => setLooping(false)}>
              {t.serialQuickSendLoopStop}
            </button>
          ) : (
            <button disabled={!connected || !draft} type="button" onClick={() => setLooping(true)}>
              <AppIcon name="play" size={12} />
              {t.serialQuickSendLoopStart}
            </button>
          )}
          <span>{t.serialQuickSendLoopHint}</span>
        </div>
        {message ? <div className="serial-quick-panel__message">{message}</div> : null}
      </div>
    </details>
  )
}
