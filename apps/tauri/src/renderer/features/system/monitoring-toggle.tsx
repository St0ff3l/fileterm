import { useEffect, useRef, useState } from 'react'
import type { SessionSnapshot, WorkspaceSnapshot } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { t } from '../../i18n'
import { waitForMonitoringRequest } from './monitoring-request'
import './monitoring-toggle.css'

export function MonitoringToggle({
  session,
  tabId,
  onSnapshot
}: {
  session: SessionSnapshot | null
  tabId: string | null
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
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  if (
    !session?.connected ||
    !state ||
    session.capabilities?.resourceMonitoring === false ||
    state.reason === 'ssh-disconnected' ||
    state.phase === 'unsupported'
  )
    return null

  async function change(enabled: boolean) {
    if (!window.fileterm || !tabId || !state || pending.current) return
    pending.current = true
    const id = ++requestId.current
    setRequested(enabled)
    setError('')
    try {
      // Apply confirmed state even if this button unmounts or the UI wait
      // times out. Workspace revisions reject stale results after reconnect.
      const request = window.fileterm.setMonitoringEnabled(tabId, state.generation, enabled).then((snapshot) => {
        onSnapshot(snapshot)
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
        title={label}
        aria-label={label}
        loading={requested !== null}
        disabled={!window.fileterm}
        icon={
          <svg
            className="system-sidebar-toggle-icon"
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M8 1.5v6M4 3.25a6 6 0 1 0 8 0" />
          </svg>
        }
        onClick={() => {
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
