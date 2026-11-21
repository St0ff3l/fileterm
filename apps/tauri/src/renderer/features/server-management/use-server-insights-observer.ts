import { useEffect, useRef } from 'react'
import type { SessionSnapshot, TabStatus, WorkspaceTab } from '@fileterm/core'
import { resolveSessionFailureDetail } from '../../hooks/workspace-tabs-utils'
import {
  flushServerInsights,
  hydrateServerInsights,
  recordConnectionEvent,
  recordServerInsight
} from './server-insights-store'

/**
 * Records server configuration snapshots and connection attempts for every
 * profile, independently of whether the server-management page is mounted.
 */
export function useServerInsightsObserver({
  tabs,
  sessions,
  enabled
}: {
  tabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  enabled: boolean
}) {
  const previousStatusesRef = useRef(new Map<string, TabStatus>())

  useEffect(() => {
    if (!enabled) {
      return
    }

    void hydrateServerInsights()
    const previousStatuses = previousStatusesRef.current
    const nextStatuses = new Map<string, TabStatus>()

    for (const tab of tabs) {
      const session = sessions[tab.id]
      const previousStatus = previousStatuses.get(tab.id)

      if (session?.connected && session.systemMetrics) {
        recordServerInsight(tab.profileId, session.systemMetrics, session.monitoring?.lastSampleAt)
      }

      if (previousStatus === 'connecting' && tab.status === 'connected') {
        recordConnectionEvent(tab.profileId, true)
      } else if (previousStatus === 'connecting' && tab.status === 'error') {
        const detail = resolveSessionFailureDetail(session?.terminalTranscript)
        recordConnectionEvent(tab.profileId, false, detail)
      }

      if (previousStatus === 'connected' && tab.status !== 'connected') {
        flushServerInsights()
      }

      nextStatuses.set(tab.id, tab.status)
    }

    previousStatusesRef.current = nextStatuses
  }, [enabled, sessions, tabs])
}
