import { useEffect, useState } from 'react'
import type { SerialTransferMode, SerialTransferProgress } from '@fileterm/core'
import { formatMessage, localizeSerialTerminalText, t } from '../../i18n'

const MODES: SerialTransferMode[] = ['raw', 'xmodem', 'ymodem']

function modeLabel(mode: SerialTransferMode) {
  if (mode === 'raw') return t.serialTransferRaw
  if (mode === 'xmodem') return t.serialTransferXmodem
  return t.serialTransferYmodem
}

function progressStatusLabel(status: SerialTransferProgress['status']) {
  if (status === 'running') return t.serialTransferProgressRunning
  if (status === 'completed') return t.serialTransferProgressCompleted
  if (status === 'failed') return t.serialTransferProgressFailed
  return t.serialTransferProgressCanceled
}

export function SerialTransferPanel({
  tabId,
  connected,
  onBusyChange
}: {
  tabId: string
  connected: boolean
  onBusyChange?: (busy: boolean) => void
}) {
  const [mode, setMode] = useState<SerialTransferMode>('ymodem')
  const [receiveDirectory, setReceiveDirectory] = useState('')
  const [receiveName, setReceiveName] = useState(t.serialTransferNamePlaceholder)
  const [busy, setBusy] = useState(false)
  const [cancelRequested, setCancelRequested] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [progress, setProgress] = useState<SerialTransferProgress | null>(null)

  useEffect(() => {
    onBusyChange?.(busy)
  }, [busy, onBusyChange])

  useEffect(() => {
    if (!window.fileterm?.onSerialTransferProgress) return
    return window.fileterm.onSerialTransferProgress((nextProgress) => {
      if (nextProgress.tabId === tabId) {
        setProgress(nextProgress)
      }
    })
  }, [tabId])

  const runSend = async () => {
    const paths = await window.fileterm?.selectLocalFiles()
    const path = paths?.[0]
    if (!path || !window.fileterm?.serialTransfer) return
    setBusy(true)
    setCancelRequested(false)
    setMessage(null)
    setProgress(null)
    try {
      const result = await window.fileterm.serialTransfer(
        tabId,
        'send',
        mode,
        path,
        undefined,
        mode === 'ymodem' ? paths : undefined
      )
      setMessage(formatMessage(t.serialTransferCompleted, { bytes: result.bytesTransferred }))
    } catch (error) {
      setMessage(`${t.serialTransferFailed}${localizeSerialTerminalText(String(error))}`)
    } finally {
      setBusy(false)
      setCancelRequested(false)
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
    setCancelRequested(false)
    setMessage(null)
    setProgress(null)
    try {
      const result = await window.fileterm.serialTransfer(
        tabId,
        'receive',
        mode,
        receiveDirectory,
        mode === 'ymodem' ? undefined : receiveName
      )
      setMessage(formatMessage(t.serialTransferCompleted, { bytes: result.bytesTransferred }))
    } catch (error) {
      setMessage(`${t.serialTransferFailed}${localizeSerialTerminalText(String(error))}`)
    } finally {
      setBusy(false)
      setCancelRequested(false)
    }
  }

  const cancelTransfer = async () => {
    if (!window.fileterm?.serialTransferCancel) return
    setCancelRequested(true)
    try {
      await window.fileterm.serialTransferCancel(tabId)
    } catch (error) {
      setCancelRequested(false)
      setMessage(`${t.serialTransferFailed}${localizeSerialTerminalText(String(error))}`)
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
          {mode === 'ymodem' ? (
            <span className="serial-transfer-panel__sender-name-hint">{t.serialTransferYmodemBatchHint}</span>
          ) : (
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
          )}
          <button disabled={!connected || busy} type="button" onClick={() => void runReceive()}>
            {t.serialTransferReceive}
          </button>
          {busy ? (
            <button disabled={cancelRequested} type="button" onClick={() => void cancelTransfer()}>
              {cancelRequested ? t.serialTransferCanceling : t.serialTransferCancel}
            </button>
          ) : null}
        </div>
        {receiveDirectory ? <div className="serial-transfer-panel__path">{receiveDirectory}</div> : null}
        <div className="serial-transfer-panel__hint">{t.serialTransferHint}</div>
        {mode === 'xmodem' ? (
          <div className="serial-transfer-panel__warning">{t.serialTransferXmodemWarning}</div>
        ) : null}
        {progress ? (
          <div className="serial-transfer-panel__progress" aria-live="polite">
            <div className="serial-transfer-panel__progress-meta">
              <span>{progressStatusLabel(progress.status)}</span>
              <span>
                {progress.bytesTransferred} / {progress.totalBytes ?? '?'} B
                {progress.speedBytesPerSecond ? ` · ${progress.speedBytesPerSecond} B/s` : ''}
              </span>
            </div>
            {progress.totalBytes ? (
              <progress max={progress.totalBytes} value={Math.min(progress.totalBytes, progress.bytesTransferred)} />
            ) : null}
            {progress.block !== undefined ? (
              <div className="serial-transfer-panel__progress-block">
                {formatMessage(t.serialTransferProgressBlock, { block: progress.block })}
              </div>
            ) : null}
            {progress.message ? (
              <div className="serial-transfer-panel__message">{localizeSerialTerminalText(progress.message)}</div>
            ) : null}
          </div>
        ) : null}
        {message ? (
          <div className="serial-transfer-panel__message" role="status">
            {message}
          </div>
        ) : null}
      </div>
    </details>
  )
}
