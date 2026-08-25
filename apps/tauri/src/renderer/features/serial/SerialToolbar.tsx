import { useCallback, useEffect, useState } from 'react'
import type { SerialLineStatus } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { localizeSerialTerminalText, t } from '../../i18n'
import { SerialTransferPanel } from './SerialTransferPanel'
import { SerialQuickSendPanel } from './SerialQuickSendPanel'

const INITIAL_STATUS: SerialLineStatus = {
  dtr: null,
  rts: null,
  cts: null,
  dsr: null,
  ring: null,
  carrierDetect: null
}

function lineValue(value: boolean | null | undefined) {
  if (value === null || value === undefined) {
    return t.serialStatusUnavailable
  }
  return value ? 'ON' : 'OFF'
}

export function SerialToolbar({
  profileId,
  tabId,
  connected
}: {
  profileId: string
  tabId: string
  connected: boolean
}) {
  const [status, setStatus] = useState<SerialLineStatus>(INITIAL_STATUS)
  const [busy, setBusy] = useState(false)
  const [transferBusy, setTransferBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const serialBusy = busy || transferBusy

  const refreshStatus = useCallback(async () => {
    if (!connected || transferBusy || !window.fileterm?.serialControl) {
      return
    }
    try {
      const next = await window.fileterm.serialControl(tabId, 'status')
      setStatus(next)
    } catch {
      setStatus(INITIAL_STATUS)
    }
  }, [connected, tabId, transferBusy])

  useEffect(() => {
    if (!connected || transferBusy) {
      setStatus(INITIAL_STATUS)
      return
    }
    void refreshStatus()
    const timer = window.setInterval(() => void refreshStatus(), 1000)
    return () => window.clearInterval(timer)
  }, [connected, refreshStatus, transferBusy])

  const runControl = useCallback(
    async (action: 'set-dtr' | 'set-rts' | 'send-break' | 'clear-buffers' | 'reset', value?: boolean) => {
      if (!connected || transferBusy || !window.fileterm?.serialControl) {
        return
      }
      setBusy(true)
      setError(null)
      try {
        const next = await window.fileterm.serialControl(
          tabId,
          action,
          value,
          action === 'send-break' ? 250 : undefined
        )
        setStatus(next)
      } catch (nextError) {
        setError(localizeSerialTerminalText(String(nextError)))
      } finally {
        setBusy(false)
      }
    },
    [connected, tabId, transferBusy]
  )

  return (
    <div className="serial-toolbar" onClick={(event) => event.stopPropagation()}>
      <div className="serial-toolbar__controls" role="group" aria-label={t.serialControlStatus}>
        <button
          aria-pressed={status.dtr === true}
          className={status.dtr === true ? 'is-active' : undefined}
          disabled={!connected || serialBusy}
          title={t.serialControlDtr}
          type="button"
          onClick={() => void runControl('set-dtr', status.dtr !== true)}
        >
          {t.serialControlDtr}
        </button>
        <button
          aria-pressed={status.rts === true}
          className={status.rts === true ? 'is-active' : undefined}
          disabled={!connected || serialBusy}
          title={t.serialControlRts}
          type="button"
          onClick={() => void runControl('set-rts', status.rts !== true)}
        >
          {t.serialControlRts}
        </button>
        <button
          disabled={!connected || serialBusy}
          title={t.serialControlBreak}
          type="button"
          onClick={() => void runControl('send-break')}
        >
          <AppIcon name="flash" size={13} />
          {t.serialControlBreak}
        </button>
        <button
          disabled={!connected || serialBusy}
          title={t.serialControlClear}
          type="button"
          onClick={() => void runControl('clear-buffers')}
        >
          {t.serialControlClear}
        </button>
        <button
          disabled={!connected || serialBusy}
          title={t.serialControlReset}
          type="button"
          onClick={() => void runControl('reset')}
        >
          {t.serialControlReset}
        </button>
        <button
          aria-label={t.serialControlStatus}
          className="serial-toolbar__refresh"
          disabled={!connected || serialBusy}
          title={t.serialControlStatus}
          type="button"
          onClick={() => void refreshStatus()}
        >
          <AppIcon name="refresh" size={13} />
        </button>
      </div>
      <div className="serial-toolbar__status" aria-live="polite">
        <span>
          {t.serialStatusDtr}: {lineValue(status.dtr)}
        </span>
        <span>
          {t.serialStatusRts}: {lineValue(status.rts)}
        </span>
        <span>
          {t.serialStatusCts}: {lineValue(status.cts)}
        </span>
        <span>
          {t.serialStatusDsr}: {lineValue(status.dsr)}
        </span>
        <span>
          {t.serialStatusRing}: {lineValue(status.ring)}
        </span>
        <span>
          {t.serialStatusCarrier}: {lineValue(status.carrierDetect)}
        </span>
      </div>
      {error ? <div className="serial-toolbar__error">{error}</div> : null}
      <SerialTransferPanel connected={connected} onBusyChange={setTransferBusy} tabId={tabId} />
      <SerialQuickSendPanel connected={connected && !transferBusy} profileId={profileId} tabId={tabId} />
    </div>
  )
}
