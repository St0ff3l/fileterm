import { useState } from 'react'
import type { SerialTransferMode } from '@fileterm/core'
import { formatMessage, localizeSerialTerminalText, t } from '../../i18n'

const MODES: SerialTransferMode[] = ['raw', 'xmodem', 'ymodem']

function modeLabel(mode: SerialTransferMode) {
  if (mode === 'raw') return t.serialTransferRaw
  if (mode === 'xmodem') return t.serialTransferXmodem
  return t.serialTransferYmodem
}

export function SerialTransferPanel({ tabId, connected }: { tabId: string; connected: boolean }) {
  const [mode, setMode] = useState<SerialTransferMode>('xmodem')
  const [receiveDirectory, setReceiveDirectory] = useState('')
  const [receiveName, setReceiveName] = useState(t.serialTransferNamePlaceholder)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string | null>(null)

  const runSend = async () => {
    const paths = await window.fileterm?.selectLocalFiles()
    const path = paths?.[0]
    if (!path || !window.fileterm?.serialTransfer) return
    setBusy(true)
    setMessage(null)
    try {
      const result = await window.fileterm.serialTransfer(tabId, 'send', mode, path)
      setMessage(formatMessage(t.serialTransferCompleted, { bytes: result.bytesTransferred }))
    } catch (error) {
      setMessage(`${t.serialTransferFailed}${localizeSerialTerminalText(String(error))}`)
    } finally {
      setBusy(false)
    }
  }

  const chooseReceiveDirectory = async () => {
    const path = await window.fileterm?.selectLocalDirectory(receiveDirectory || undefined)
    if (path) setReceiveDirectory(path)
  }

  const runReceive = async () => {
    if (!receiveDirectory) {
      setMessage(t.serialTransferDirectoryMissing)
      return
    }
    if (!window.fileterm?.serialTransfer) return
    setBusy(true)
    setMessage(null)
    try {
      const result = await window.fileterm.serialTransfer(tabId, 'receive', mode, receiveDirectory, receiveName)
      setMessage(formatMessage(t.serialTransferCompleted, { bytes: result.bytesTransferred }))
    } catch (error) {
      setMessage(`${t.serialTransferFailed}${localizeSerialTerminalText(String(error))}`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <details className="serial-transfer-panel">
      <summary>{t.serialTransfer}</summary>
      <div className="serial-transfer-panel__body">
        <div className="serial-transfer-panel__modes" role="group" aria-label={t.serialTransfer}>
          {MODES.map((nextMode) => (
            <button
              aria-pressed={mode === nextMode}
              className={mode === nextMode ? 'is-active' : undefined}
              disabled={!connected || busy}
              key={nextMode}
              type="button"
              onClick={() => setMode(nextMode)}
            >
              {modeLabel(nextMode)}
            </button>
          ))}
        </div>
        <div className="serial-transfer-panel__row">
          <button disabled={!connected || busy} type="button" onClick={() => void runSend()}>
            {t.serialTransferSend}
          </button>
          <button disabled={busy} type="button" onClick={() => void chooseReceiveDirectory()}>
            {t.serialTransferChooseDirectory}
          </button>
          <label>
            {t.serialTransferName}
            <input
              disabled={busy}
              placeholder={t.serialTransferNamePlaceholder}
              spellCheck={false}
              value={receiveName}
              onChange={(event) => setReceiveName(event.target.value)}
            />
          </label>
          <button disabled={!connected || busy} type="button" onClick={() => void runReceive()}>
            {t.serialTransferReceive}
          </button>
        </div>
        {receiveDirectory ? <div className="serial-transfer-panel__path">{receiveDirectory}</div> : null}
        <div className="serial-transfer-panel__hint">{t.serialTransferHint}</div>
        {message ? (
          <div className="serial-transfer-panel__message" role="status">
            {message}
          </div>
        ) : null}
      </div>
    </details>
  )
}
