import { DIAGNOSTIC_SCOPES, writeDiagnosticLog } from '../../lib/diagnostic-log'
import { useEffect, useRef } from 'react'
import type { SessionSnapshot, TabStatus } from '@fileterm/core'

function writeMonitoringDiagnostic(message: string) {
  writeDiagnosticLog('INFO', DIAGNOSTIC_SCOPES.monitoring, message)
}

/** Refs #259: retain useful state and sample summaries without remote command output. */
export function useMonitoringDiagnostics({
  tabId,
  session,
  collapsed,
  connectionStatus
}: {
  tabId: string | null
  session: SessionSnapshot | null
  collapsed: boolean
  connectionStatus: TabStatus | null
}) {
  const previousState = useRef('')
  const previousRender = useRef({ key: '', at: 0 })
  const previousSample = useRef({ key: '', at: 0 })
  const state = session?.monitoring
  const metrics = session?.systemMetrics
  const summary = `tab_id=${tabId ?? 'none'} connected=${session?.connected === true} connection_status=${connectionStatus ?? 'none'} collapsed=${collapsed} capability=${session?.capabilities?.resourceMonitoring ?? 'unknown'} phase=${state?.phase ?? 'none'} reason=${state?.reason ?? 'none'} generation=${state?.generation ?? 'none'} attempt=${state?.attempt ?? 0} has_metrics=${Boolean(metrics)}`

  useEffect(() => {
    if (summary === previousState.current) return
    previousState.current = summary
    writeMonitoringDiagnostic(`sidebar rendered ${summary}`)
  }, [summary])

  useEffect(() => {
    if (!tabId || state?.phase !== 'healthy') return
    const key = `${tabId}:${state.generation}`
    const now = Date.now()
    if (previousRender.current.key === key && now - previousRender.current.at < 10_000) return
    previousRender.current = { key, at: now }
    const age = state.lastSampleAt === undefined ? 'unknown' : Math.max(0, now - state.lastSampleAt)
    writeMonitoringDiagnostic(
      `sidebar metrics rendered tab_id=${tabId} generation=${state.generation} revision=${state.revision} sample_age_ms=${age}`
    )
  }, [state?.generation, state?.lastSampleAt, state?.phase, state?.revision, tabId])

  useEffect(() => {
    if (!tabId || !metrics || state?.phase !== 'healthy') return
    const key = `${tabId}:${state.generation}`
    const now = Date.now()
    if (previousSample.current.key === key && now - previousSample.current.at < 60_000) return
    previousSample.current = { key, at: now }
    const processCpu = (metrics.topProcesses ?? []).map((row) => Number.parseFloat(row.cpu)).filter(Number.isFinite)
    const cores = (metrics.cpuInfoRows ?? []).reduce((total, row) => total + row.cores, 0)
    const age = state.lastSampleAt === undefined ? 'unknown' : Math.max(0, now - state.lastSampleAt)
    writeMonitoringDiagnostic(
      `sample summary tab_id=${tabId} generation=${state.generation} revision=${state.revision} platform=${metrics.platform ?? 'unknown'} sample_age_ms=${age} cpu_percent=${metrics.cpuPercent} logical_cores=${cores} process_rows=${metrics.topProcesses?.length ?? 0} process_cpu_sum=${processCpu.reduce((total, cpu) => total + cpu, 0).toFixed(2)} process_cpu_max=${Math.max(0, ...processCpu).toFixed(2)} filesystem_rows=${metrics.fileSystemRows?.length ?? 0} network_rows=${metrics.networkInterfaceRows?.length ?? 0}`
    )
  }, [metrics, state, tabId])

  return (action: 'toggle-sidebar' | 'open-system-info') => {
    const request = action === 'toggle-sidebar' ? ` requested_collapsed=${!collapsed}` : ''
    writeMonitoringDiagnostic(`sidebar action action=${action}${request} ${summary}`)
  }
}
