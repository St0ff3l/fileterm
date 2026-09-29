import { useEffect, useRef, useState } from 'react'
import type { SessionSnapshot, TabStatus, WorkspaceSnapshot } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { formatMessage, t } from '../../i18n'
import './monitoring-overlay.css'
import { waitForMonitoringRequest } from './monitoring-request'

export function monitoringIsObscured(session: SessionSnapshot | null): boolean {
  return Boolean(
    session?.capabilities?.resourceMonitoring !== false &&
    session?.monitoring &&
    (session.connected === false || session.monitoring.phase !== 'healthy')
  )
}

export function MonitoringOverlay({
  session,
  tabId,
  collapsed,
  connectionStatus,
  onSnapshot
}: {
  session: SessionSnapshot | null
  tabId: string | null
  collapsed: boolean
  connectionStatus?: TabStatus | null
  onSnapshot(snapshot: WorkspaceSnapshot): void
}) {
  const state = session?.monitoring
  const visible = monitoringIsObscured(session)
  const [now, setNow] = useState(Date.now)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const pending = useRef(false)
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => {
    if (!visible || collapsed) return
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [visible, collapsed])
  if (!visible || !state) return null
  const sshDisconnected = session?.connected === false || state.reason === 'ssh-disconnected'
  const reconnecting = connectionStatus === 'connecting'
  const phase = reconnecting ? 'reconnecting' : sshDisconnected ? 'ssh-disconnected' : state.phase
  const titles = {
    starting: t.monitoringStarting,
    stopped: t.monitoringStopped,
    healthy: '',
    paused: t.monitoringPaused,
    waiting: t.monitoringWaitingRetry,
    recovering: t.monitoringRecovering,
    failed: t.monitoringFailed,
    disconnected: t.monitoringDisconnected,
    'ssh-disconnected': t.monitoringSshDisconnected,
    reconnecting: t.monitoringSshReconnecting,
    unsupported: t.monitoringUnsupported
  }
  const title = titles[phase]
  if (collapsed)
    return (
      <span className="monitoring-collapsed" role="status" title={title} aria-label={title}>
        !
      </span>
    )
  const canRetry = ['paused', 'waiting', 'failed', 'disconnected', 'ssh-disconnected'].includes(phase)
  const busy = phase === 'starting' || phase === 'recovering' || reconnecting || submitting
  async function retry() {
    const api = window.fileterm
    if (!api || !tabId || !state || !canRetry || pending.current) return
    pending.current = true
    setSubmitting(true)
    setError('')
    try {
      if (sshDisconnected) {
        await waitForMonitoringRequest(
          api.reconnectTab(tabId).then((snapshot) => {
            onSnapshot(snapshot)
            if (
              !snapshot.tabs.some(
                (tab) => tab.id === tabId && (tab.status === 'connecting' || tab.status === 'connected')
              )
            ) {
              throw new Error('Session reconnect was not started')
            }
          })
        )
      } else {
        await waitForMonitoringRequest(api.retryMonitoring(tabId, state.generation))
      }
    } catch {
      if (mounted.current) setError(sshDisconnected ? t.monitoringReconnectError : t.monitoringRetryError)
    } finally {
      pending.current = false
      if (mounted.current) setSubmitting(false)
    }
  }
  return (
    <div className="monitoring-overlay" data-phase={phase}>
      <div className="monitoring-overlay-message">
        <strong role="status" aria-live="polite">
          {title}
        </strong>
        {phase === 'stopped' ? <p>{t.monitoringStoppedDescription}</p> : null}
        {phase === 'paused' ? <p>{t.monitoringWaitingChannel}</p> : null}
        {phase === 'ssh-disconnected' ? <p>{t.monitoringReconnectDescription}</p> : null}
        {state.lastSampleAt !== undefined ? (
          <>
            <p>
              {formatMessage(t.monitoringSampleAge, {
                seconds: Math.max(0, Math.floor((now - state.lastSampleAt) / 1000))
              })}
            </p>
            <p>{formatMessage(t.monitoringLastUpdate, { time: new Date(state.lastSampleAt).toLocaleTimeString() })}</p>
          </>
        ) : null}
        {state.nextRetryAt !== undefined && !sshDisconnected && canRetry ? (
          <p>
            {formatMessage(t.monitoringRetryCountdown, {
              seconds: Math.max(0, Math.ceil((state.nextRetryAt - now) / 1000)),
              attempt: state.attempt + 1,
              max: state.maxAttempts
            })}
          </p>
        ) : null}
        {phase === 'recovering' && state.attempt > 0 ? (
          <p>{formatMessage(t.monitoringAttempt, { attempt: state.attempt, max: state.maxAttempts })}</p>
        ) : null}
        {phase === 'starting' ? <p>{t.monitoringWaitingSample}</p> : null}
        {phase === 'recovering' ? <p>{t.monitoringRecoveryDescription}</p> : null}
        {phase === 'failed' ? <p>{formatMessage(t.monitoringExhausted, { max: state.maxAttempts })}</p> : null}
        {phase !== 'unsupported' && phase !== 'stopped' ? (
          <Button size="sm" disabled={!canRetry || !window.fileterm} loading={busy} onClick={() => void retry()}>
            {sshDisconnected || reconnecting
              ? busy
                ? t.monitoringSshReconnecting
                : t.monitoringReconnect
              : phase === 'starting'
                ? t.monitoringStarting
                : busy
                  ? t.monitoringRetryBusy
                  : t.monitoringRetry}
          </Button>
        ) : null}
        {error ? <p role="alert">{error}</p> : null}
      </div>
    </div>
  )
}
