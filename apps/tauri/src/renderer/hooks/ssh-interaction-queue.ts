import type { SshInteractionRequest } from '@fileterm/core'

export type SshInteractionQueueState = {
  /** Requests grouped by the connection attempt that owns them. */
  flows: Map<string, SshInteractionRequest[]>
  /** Flow order for the one-visible-modal scheduler. */
  readyFlowIds: string[]
  activeRequestId: string | null
}

export function createSshInteractionQueue(): SshInteractionQueueState {
  return {
    flows: new Map(),
    readyFlowIds: [],
    activeRequestId: null
  }
}

function requestFlowId(request: SshInteractionRequest): string {
  // The backend always sends flowId. The tab fallback keeps an already-open
  // renderer from becoming unusable while an older backend is being replaced.
  return request.flowId || request.tabId
}

function requestInQueue(flows: Map<string, SshInteractionRequest[]>, requestId: string) {
  for (const [flowId, requests] of flows) {
    const index = requests.findIndex((request) => request.requestId === requestId)
    if (index !== -1) return { flowId, index }
  }
  return null
}

function firstRequestId(flows: Map<string, SshInteractionRequest[]>, readyFlowIds: string[]) {
  for (const flowId of readyFlowIds) {
    const request = flows.get(flowId)?.[0]
    if (request) return request.requestId
  }
  return null
}

function normalizeQueue(
  flows: Map<string, SshInteractionRequest[]>,
  readyFlowIds: string[],
  activeRequestId: string | null
): SshInteractionQueueState {
  const nextReadyFlowIds: string[] = []
  const seen = new Set<string>()
  for (const flowId of readyFlowIds) {
    if (!seen.has(flowId) && flows.get(flowId)?.length) {
      seen.add(flowId)
      nextReadyFlowIds.push(flowId)
    }
  }
  for (const flowId of flows.keys()) {
    if (!seen.has(flowId) && flows.get(flowId)?.length) {
      seen.add(flowId)
      nextReadyFlowIds.push(flowId)
    }
  }

  const hasActiveRequest = activeRequestId
    ? [...flows.values()].some((requests) => requests.some((request) => request.requestId === activeRequestId))
    : false
  return {
    flows,
    readyFlowIds: nextReadyFlowIds,
    activeRequestId: hasActiveRequest ? activeRequestId : firstRequestId(flows, nextReadyFlowIds)
  }
}

export function enqueueSshInteraction(
  state: SshInteractionQueueState,
  request: SshInteractionRequest
): SshInteractionQueueState {
  const flows = new Map(state.flows)
  const readyFlowIds = [...state.readyFlowIds]
  const existing = requestInQueue(flows, request.requestId)
  const flowId = requestFlowId(request)

  if (existing) {
    const existingRequests = [...(flows.get(existing.flowId) ?? [])]
    existingRequests.splice(existing.index, 1)
    if (existing.flowId === flowId) {
      existingRequests.splice(existing.index, 0, request)
      flows.set(flowId, existingRequests)
    } else {
      if (existingRequests.length) {
        flows.set(existing.flowId, existingRequests)
      } else {
        flows.delete(existing.flowId)
        const readyIndex = readyFlowIds.indexOf(existing.flowId)
        if (readyIndex !== -1) readyFlowIds.splice(readyIndex, 1)
      }
      flows.set(flowId, [...(flows.get(flowId) ?? []), request])
      if (!readyFlowIds.includes(flowId)) readyFlowIds.push(flowId)
    }
  } else {
    flows.set(flowId, [...(flows.get(flowId) ?? []), request])
    if (!readyFlowIds.includes(flowId)) readyFlowIds.push(flowId)
  }

  return normalizeQueue(flows, readyFlowIds, state.activeRequestId)
}

export function removeSshInteraction(state: SshInteractionQueueState, requestId: string): SshInteractionQueueState {
  const location = requestInQueue(state.flows, requestId)
  if (!location) return state

  const flows = new Map(state.flows)
  const requests = [...(flows.get(location.flowId) ?? [])]
  requests.splice(location.index, 1)
  const readyFlowIds = [...state.readyFlowIds]
  const wasActive = state.activeRequestId === requestId
  if (requests.length) {
    flows.set(location.flowId, requests)
    // Rotate a flow after its visible request is acknowledged. This keeps a
    // busy connection from starving prompts belonging to another flow while
    // preserving strict order inside each individual flow.
    if (wasActive) {
      const readyIndex = readyFlowIds.indexOf(location.flowId)
      if (readyIndex !== -1) {
        readyFlowIds.splice(readyIndex, 1)
        readyFlowIds.push(location.flowId)
      }
    }
  } else {
    flows.delete(location.flowId)
    const readyIndex = readyFlowIds.indexOf(location.flowId)
    if (readyIndex !== -1) readyFlowIds.splice(readyIndex, 1)
  }

  return normalizeQueue(flows, readyFlowIds, wasActive ? null : state.activeRequestId)
}

export function clearSshInteractionFlow(state: SshInteractionQueueState, flowId: string): SshInteractionQueueState {
  if (!state.flows.has(flowId)) return state
  const flows = new Map(state.flows)
  const flowRequests = flows.get(flowId) ?? []
  flows.delete(flowId)
  const readyFlowIds = state.readyFlowIds.filter((id) => id !== flowId)
  const activeBelongsToFlow = flowRequests.some((request) => request.requestId === state.activeRequestId)
  return normalizeQueue(flows, readyFlowIds, activeBelongsToFlow ? null : state.activeRequestId)
}

export function getActiveSshInteraction(state: SshInteractionQueueState): SshInteractionRequest | null {
  if (!state.activeRequestId) return null
  for (const requests of state.flows.values()) {
    const request = requests.find((candidate) => candidate.requestId === state.activeRequestId)
    if (request) return request
  }
  return null
}
