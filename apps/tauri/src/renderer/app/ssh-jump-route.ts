import type { ConnectionProfile, ProxyConfig, SshProfile } from '@fileterm/core'

export type SshJumpRouteError = 'cycle' | 'missing' | 'type' | 'too-long' | 'proxy-conflict' | 'interface-conflict'

export function getSshRoutingMode(target: { jumpProfileId?: string; jumpProfileIds?: string[] }): 'direct' | 'jump' {
  if (target.jumpProfileIds !== undefined) return target.jumpProfileIds.length ? 'jump' : 'direct'
  return target.jumpProfileId ? 'jump' : 'direct'
}

export function hasIncompleteSshJumpRoute(target: { jumpProfileIds?: string[] }): boolean {
  return target.jumpProfileIds?.some((id) => !id.trim()) ?? false
}

export function resolveSshJumpRoute(
  target: {
    id?: string
    jumpProfileId?: string
    jumpProfileIds?: string[]
    proxy?: ProxyConfig
    proxyProfileId?: string
  },
  profiles: ConnectionProfile[]
): { jumpHosts: SshProfile[]; error: SshJumpRouteError | null } {
  const jumpHosts: SshProfile[] = []
  if (target.jumpProfileIds !== undefined) {
    if (!Array.isArray(target.jumpProfileIds)) return { jumpHosts: [], error: 'missing' }
    if (target.jumpProfileIds.length > 8) return { jumpHosts: [], error: 'too-long' }
    const hasProxy = Boolean(target.proxyProfileId) || Boolean(target.proxy?.type && target.proxy.type !== 'none')
    if (target.jumpProfileIds.length && hasProxy) return { jumpHosts: [], error: 'proxy-conflict' }

    const visited = new Set(target.id ? [target.id] : [])
    for (const [index, id] of target.jumpProfileIds.entries()) {
      if (typeof id !== 'string' || !id.trim()) return { jumpHosts: [], error: 'missing' }
      if (visited.has(id)) return { jumpHosts: [], error: 'cycle' }
      visited.add(id)
      const hop = profiles.find((profile) => profile.id === id)
      if (!hop) return { jumpHosts: [], error: 'missing' }
      if (hop.type !== 'ssh') return { jumpHosts: [], error: 'type' }
      if (hop.sshNetworkInterface) return { jumpHosts: [], error: 'interface-conflict' }
      const hopHasProxy = Boolean(hop.proxyProfileId) || Boolean(hop.proxy?.type && hop.proxy.type !== 'none')
      if (index > 0 && hopHasProxy) return { jumpHosts: [], error: 'proxy-conflict' }
      jumpHosts.push(hop)
    }
    return { jumpHosts, error: null }
  }

  const visited = new Set(target.id ? [target.id] : [])
  let current = target
  while (current.jumpProfileId) {
    if (current.proxyProfileId || (current.proxy?.type && current.proxy.type !== 'none')) {
      return { jumpHosts: [], error: 'proxy-conflict' }
    }
    const id = current.jumpProfileId
    if (visited.has(id)) return { jumpHosts: [], error: 'cycle' }
    if (jumpHosts.length >= 8) return { jumpHosts: [], error: 'too-long' }
    visited.add(id)
    const hop = profiles.find((profile) => profile.id === id)
    if (!hop) return { jumpHosts: [], error: 'missing' }
    if (hop.type !== 'ssh') return { jumpHosts: [], error: 'type' }
    if (hop.sshNetworkInterface) return { jumpHosts: [], error: 'interface-conflict' }
    jumpHosts.push(hop)
    current = hop
  }
  return { jumpHosts: jumpHosts.reverse(), error: null }
}
