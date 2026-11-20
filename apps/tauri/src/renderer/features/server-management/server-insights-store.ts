import type { SystemMetrics } from '@fileterm/core'

/**
 * Renderer-local cache for the server-management overview.
 *
 * Profile IDs remain stable across renderer sessions, so both the last known
 * server configuration and the connection history are keyed by profile ID in
 * the shared UI state store.
 */
const SERVER_INSIGHTS_UI_STATE_KEY = 'server-management.insights.v1'
const CONNECTION_HISTORY_UI_STATE_KEY = 'server-management.connection-history.v1'

const HISTORY_LIMIT = 12
const INSIGHT_PERSIST_DELAY_MS = 5000

export interface ServerInsight {
  updatedAt: number
  osName?: string
  architecture?: string
  hostname?: string
  kernelVersion?: string
  uptime?: string
  uptimeSeconds?: number
  cpuModel?: string
  cpuCores?: number
  memoryTotal?: string
  diskUsed?: string
  diskSize?: string
  diskPercent?: number | null
}

export interface ConnectionHistoryEntry {
  id: string
  at: number
  ok: boolean
  error?: string
}

type ServerInsightsListener = () => void

const insights = new Map<string, ServerInsight>()
const history = new Map<string, ConnectionHistoryEntry[]>()
const localOverrides = new Set<string>()
const listeners = new Set<ServerInsightsListener>()

let hasHydrated = false
let hydrationPromise: Promise<void> | null = null
let persistInFlight = false
let pendingInsightsWrite = false
let pendingHistoryWrite = false
let insightPersistTimer: ReturnType<typeof setTimeout> | null = null

function getDesktopApi() {
  return typeof window === 'undefined' ? undefined : window.fileterm
}

function notifyListeners() {
  for (const listener of listeners) {
    listener()
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null
}

function optionalString(value: unknown) {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined
}

function optionalNumber(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function parsePercent(value: string | undefined) {
  const parsed = Number.parseFloat(value ?? '')
  return Number.isFinite(parsed) ? Math.max(0, Math.min(100, parsed)) : null
}

function parseInsights(raw: string | null) {
  const parsed = asRecord(raw ? JSON.parse(raw) : null)
  const result = new Map<string, ServerInsight>()
  if (!parsed) {
    return result
  }

  for (const [profileId, value] of Object.entries(parsed)) {
    const entry = asRecord(value)
    const updatedAt = optionalNumber(entry?.updatedAt)
    if (!profileId || !entry || !updatedAt) {
      continue
    }
    const insight: ServerInsight = {
      updatedAt,
      osName: optionalString(entry.osName),
      architecture: optionalString(entry.architecture),
      hostname: optionalString(entry.hostname),
      kernelVersion: optionalString(entry.kernelVersion),
      uptime: optionalString(entry.uptime),
      uptimeSeconds: optionalNumber(entry.uptimeSeconds),
      cpuModel: optionalString(entry.cpuModel),
      cpuCores: optionalNumber(entry.cpuCores),
      memoryTotal: optionalString(entry.memoryTotal),
      diskUsed: optionalString(entry.diskUsed),
      diskSize: optionalString(entry.diskSize),
      diskPercent: optionalNumber(entry.diskPercent) ?? null
    }
    if (
      insight.osName ||
      insight.uptime ||
      insight.uptimeSeconds ||
      insight.cpuModel ||
      insight.memoryTotal ||
      insight.diskSize
    ) {
      result.set(profileId, insight)
    }
  }

  return result
}

function parseHistory(raw: string | null) {
  const parsed = asRecord(raw ? JSON.parse(raw) : null)
  const result = new Map<string, ConnectionHistoryEntry[]>()
  if (!parsed) {
    return result
  }

  for (const [profileId, value] of Object.entries(parsed)) {
    if (!profileId || !Array.isArray(value)) {
      continue
    }
    const entries = value
      .map((item, index): ConnectionHistoryEntry | null => {
        const entry = asRecord(item)
        const at = optionalNumber(entry?.at)
        if (!entry || !at || typeof entry.ok !== 'boolean') {
          return null
        }
        return {
          id: optionalString(entry.id) ?? `${at}-${index}`,
          at,
          ok: entry.ok,
          error: optionalString(entry.error)
        }
      })
      .filter((entry): entry is ConnectionHistoryEntry => entry !== null)
      .slice(0, HISTORY_LIMIT)
    if (entries.length) {
      result.set(profileId, entries)
    }
  }

  return result
}

function serializeInsights() {
  return JSON.stringify(Object.fromEntries(insights))
}

function serializeHistory() {
  return JSON.stringify(Object.fromEntries(history))
}

async function flushPersistQueue() {
  const desktopApi = getDesktopApi()
  if (!desktopApi || persistInFlight) {
    return
  }
  if (!hasHydrated) {
    await hydrateServerInsights()
  }

  persistInFlight = true
  try {
    while (pendingInsightsWrite || pendingHistoryWrite) {
      const writeInsights = pendingInsightsWrite
      const writeHistory = pendingHistoryWrite
      pendingInsightsWrite = false
      pendingHistoryWrite = false
      try {
        if (writeInsights) {
          await desktopApi.setUiStateItem(SERVER_INSIGHTS_UI_STATE_KEY, serializeInsights())
        }
        if (writeHistory) {
          await desktopApi.setUiStateItem(CONNECTION_HISTORY_UI_STATE_KEY, serializeHistory())
        }
      } catch {
        // A failed cache write must not interrupt the connection workflow. The
        // in-memory value stays authoritative for this renderer session.
      }
    }
  } finally {
    persistInFlight = false
  }
}

function scheduleInsightPersist() {
  pendingInsightsWrite = true
  if (!getDesktopApi() || insightPersistTimer) {
    return
  }

  insightPersistTimer = setTimeout(() => {
    insightPersistTimer = null
    void flushPersistQueue()
  }, INSIGHT_PERSIST_DELAY_MS)
}

function requestHistoryPersist() {
  pendingHistoryWrite = true
  void flushPersistQueue()
}

export function serverInsightFromMetrics(metrics: SystemMetrics, sampledAt?: number): ServerInsight {
  const cpuRow = metrics.cpuInfoRows?.[0]
  const rootFileSystem = metrics.fileSystemRows?.find((row) => row.mountPoint === '/') ?? metrics.fileSystemRows?.[0]

  return {
    updatedAt: sampledAt && Number.isFinite(sampledAt) ? sampledAt : Date.now(),
    osName: optionalString(metrics.identity?.osName),
    architecture: optionalString(metrics.identity?.architecture),
    hostname: optionalString(metrics.identity?.hostname),
    kernelVersion: optionalString(metrics.identity?.kernelVersion),
    uptime: optionalString(metrics.uptime),
    uptimeSeconds: optionalNumber(metrics.uptimeSeconds),
    cpuModel: optionalString(cpuRow?.model),
    cpuCores: cpuRow?.cores && cpuRow.cores > 0 ? cpuRow.cores : undefined,
    memoryTotal: optionalString(metrics.memoryBreakdown?.total),
    diskUsed: optionalString(rootFileSystem?.used),
    diskSize: optionalString(rootFileSystem?.size),
    diskPercent: parsePercent(rootFileSystem?.usagePercent)
  }
}

/** Store the latest non-empty server configuration snapshot for a profile. */
export function recordServerInsight(profileId: string, metrics: SystemMetrics, sampledAt?: number) {
  if (!profileId) {
    return
  }

  const next = serverInsightFromMetrics(metrics, sampledAt)
  if (!next.osName && !next.uptime && !next.uptimeSeconds && !next.cpuModel && !next.memoryTotal && !next.diskSize) {
    return
  }

  const previous = insights.get(profileId)
  const changed =
    !previous ||
    previous.osName !== next.osName ||
    previous.architecture !== next.architecture ||
    previous.hostname !== next.hostname ||
    previous.kernelVersion !== next.kernelVersion ||
    previous.uptime !== next.uptime ||
    previous.uptimeSeconds !== next.uptimeSeconds ||
    previous.cpuModel !== next.cpuModel ||
    previous.cpuCores !== next.cpuCores ||
    previous.memoryTotal !== next.memoryTotal ||
    previous.diskUsed !== next.diskUsed ||
    previous.diskSize !== next.diskSize ||
    previous.diskPercent !== next.diskPercent

  insights.set(profileId, next)
  localOverrides.add(profileId)
  if (changed) {
    scheduleInsightPersist()
  }
}

/** Append a successful or failed connection attempt to the profile history. */
export function recordConnectionEvent(profileId: string, ok: boolean, error?: string) {
  if (!profileId) {
    return
  }

  const at = Date.now()
  const entry: ConnectionHistoryEntry = {
    id: `${at}-${Math.random().toString(36).slice(2, 8)}`,
    at,
    ok,
    error: error?.trim() || undefined
  }
  const entries = [entry, ...(history.get(profileId) ?? [])].slice(0, HISTORY_LIMIT)
  history.set(profileId, entries)
  localOverrides.add(profileId)
  notifyListeners()
  requestHistoryPersist()
}

export function getServerInsight(profileId: string) {
  return insights.get(profileId)
}

export function getConnectionHistory(profileId: string) {
  return history.get(profileId) ?? []
}

export function subscribeServerInsights(listener: ServerInsightsListener) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** Immediately write pending caches, e.g. when a session disconnects. */
export function flushServerInsights() {
  if (insightPersistTimer) {
    clearTimeout(insightPersistTimer)
    insightPersistTimer = null
  }
  pendingInsightsWrite = true
  pendingHistoryWrite = true
  void flushPersistQueue()
}

export function hydrateServerInsights() {
  if (hasHydrated) {
    return Promise.resolve()
  }
  if (hydrationPromise) {
    return hydrationPromise
  }

  const desktopApi = getDesktopApi()
  if (!desktopApi) {
    hasHydrated = true
    return Promise.resolve()
  }

  hydrationPromise = Promise.all([
    desktopApi.getUiStateItem(SERVER_INSIGHTS_UI_STATE_KEY),
    desktopApi.getUiStateItem(CONNECTION_HISTORY_UI_STATE_KEY)
  ])
    .then(([rawInsights, rawHistory]) => {
      try {
        for (const [profileId, insight] of parseInsights(rawInsights)) {
          if (!localOverrides.has(profileId)) {
            insights.set(profileId, insight)
          }
        }
      } catch {
        // Ignore malformed insight JSON and keep the in-memory cache.
      }
      try {
        for (const [profileId, entries] of parseHistory(rawHistory)) {
          if (!localOverrides.has(profileId)) {
            history.set(profileId, entries)
          }
        }
      } catch {
        // Ignore malformed history JSON and keep the in-memory cache.
      }
    })
    .catch(() => {
      // Keep in-memory values when the persisted cache is unavailable.
    })
    .finally(() => {
      hasHydrated = true
      notifyListeners()
    })

  return hydrationPromise
}
