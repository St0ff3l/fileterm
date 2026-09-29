import { mergeSystemMetricsHistory, type SessionMetricsUpdate, type WorkspaceSnapshot } from '@fileterm/core'

/** Metrics and full snapshots can arrive through different IPC queues. */
export function mergeMonitoringSnapshot(current: WorkspaceSnapshot, incoming: WorkspaceSnapshot): WorkspaceSnapshot {
  const sessions = { ...incoming.sessions }
  for (const [tabId, next] of Object.entries(sessions)) {
    const previous = current.sessions[tabId]
    const a = previous?.monitoring
    const b = next.monitoring
    if (a && b && (a.generation > b.generation || (a.generation === b.generation && a.revision > b.revision))) {
      sessions[tabId] = {
        ...next,
        monitoring: a,
        systemMetrics: previous.systemMetrics,
        ...(a.phase === 'unsupported' && next.capabilities
          ? { capabilities: { ...next.capabilities, resourceMonitoring: false } }
          : {})
      }
    }
  }
  return { ...incoming, sessions }
}

export function applyMonitoringUpdate(current: WorkspaceSnapshot, update: SessionMetricsUpdate): WorkspaceSnapshot {
  const session = current.sessions[update.tabId]
  if (!session) return current
  const previous = session.monitoring
  const next = update.monitoring
  if (
    next &&
    (!session.connected ||
      (previous &&
        (next.generation < previous.generation ||
          (next.generation === previous.generation && next.revision <= previous.revision))))
  ) {
    return current
  }
  // An unversioned event must never overwrite a supervised collector.
  if (previous && !next) return current
  // A versioned session is installed by its connected snapshot before accepting events.
  if (next && !previous) return current
  const metrics =
    update.systemMetrics === undefined
      ? session.systemMetrics
      : update.mode === 'append'
        ? mergeSystemMetricsHistory(session.systemMetrics, update.systemMetrics)
        : update.systemMetrics
  return {
    ...current,
    sessions: {
      ...current.sessions,
      [update.tabId]: {
        ...session,
        systemMetrics: metrics,
        monitoring: next ?? previous,
        ...(next?.phase === 'unsupported'
          ? { capabilities: session.capabilities ? { ...session.capabilities, resourceMonitoring: false } : undefined }
          : {})
      }
    }
  }
}
