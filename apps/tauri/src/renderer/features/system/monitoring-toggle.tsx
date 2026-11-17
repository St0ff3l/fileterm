import { useEffect, useRef, useState } from 'react'
import type { SessionSnapshot, WorkspaceSnapshot } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { t } from '../../i18n'
import { waitForMonitoringRequest } from './monitoring-request'
import type { MonitoringTarget } from './shared-monitoring'
import './monitoring-toggle.css'

export function MonitoringToggle({
  session,
  tabId,
  peers = [],
  onSnapshot
}: {
  session: SessionSnapshot | null
  tabId: string | null
  /** Sessions sharing this machine's collector; stop/start applies to all of them. */
  peers?: MonitoringTarget[]
  onSnapshot(snapshot: WorkspaceSnapshot): void
}) {
  const [confirming, setConfirming] = useState(false)
  const [requested, setRequested] = useState<boolean | null>(null)
  const [error, setError] = useState('')
  const pending = useRef(false)
  const requestId = useRef(0)
  const mounted = useRef(true)
  const state = session?.monitoring
  const stopped = state?.phase === 'stopped'
  const disconnected = !session?.connected || state?.reason === 'ssh-disconnected'
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => {
    if (disconnected) {
      requestId.current += 1
      setConfirming(false)
      setError('')
    }
  }, [disconnected])
  if (!session || !state || session.capabilities?.resourceMonitoring === false || state.phase === 'unsupported')
    return null

  async function change(enabled: boolean) {
    if (!window.fileterm || !tabId || !state || disconnected || pending.current) return
    pending.current = true
    const id = ++requestId.current
    setRequested(enabled)
    setError('')
    const api = window.fileterm
    const targets = (peers.length ? peers : [{ tabId, session: session as SessionSnapshot }])
      .filter((target) => target.session.monitoring && (target.session.monitoring.phase === 'stopped') === enabled)
      // Stop the collector holding the lease last so no follower takes over in between.
      .sort((a, b) => Number(a.tabId === tabId) - Number(b.tabId === tabId))
    try {
      // Apply confirmed state even if this button unmounts or the UI wait
      // times out. Workspace revisions reject stale results after reconnect.
      const request = targets
        .reduce<Promise<void>>(
          (previous, target) =>
            previous.then(() =>
              api
                .setMonitoringEnabled(target.tabId, target.session.monitoring?.generation ?? 0, enabled)
                .then(onSnapshot)
            ),
          Promise.resolve()
        )
        .then(() => {
          if (mounted.current && requestId.current === id) {
            setError('')
            setConfirming(false)
          }
        })
      await waitForMonitoringRequest(request)
    } catch {
      if (mounted.current && requestId.current === id) {
        setRequested(null)
        setError(t.monitoringControlError)
      }
    } finally {
      pending.current = false
      if (mounted.current) setRequested(null)
    }
  }
  const label = stopped ? t.monitoringStart : t.monitoringStop
  return (
    <>
      <Button
        className="monitoring-toggle"
        data-stopped={stopped}
        size="sm"
        title={disconnected ? t.monitoringSshDisconnected : label}
        aria-label={disconnected ? t.monitoringSshDisconnected : label}
        loading={requested !== null}
        reserveLoadingSpace
        disabled={!window.fileterm || !tabId || disconnected}
        icon={<span className="monitoring-toggle-icon" aria-hidden="true" />}
        onClick={() => {
          if (disconnected) return
          setError('')
          if (stopped) void change(true)
          else {
            requestId.current += 1
            setConfirming(true)
          }
        }}
      />
      {error && !confirming ? (
        <span className="monitoring-control-error" role="alert">
          {error}
        </span>
      ) : null}
      {confirming ? (
        <ConfirmActionDialog
          title={t.monitoringStop}
          description={t.monitoringStopDescription}
          confirmLabel={t.monitoringStop}
          isSubmitting={requested !== null}
          errorMessage={error}
          onClose={() => {
            requestId.current += 1
            setConfirming(false)
          }}
          onConfirm={() => void change(false)}
        />
      ) : null}
    </>
  )
}
