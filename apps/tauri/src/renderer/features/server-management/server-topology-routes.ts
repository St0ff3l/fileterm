import type { ConnectionProfile, ProxyProfile } from '@fileterm/core'
import { resolveSshJumpRoute } from '../../app/ssh-jump-route'

const PROXY_NODE_PREFIX = 'topology-proxy:'

export type TopologyProxyNode = Pick<ProxyProfile, 'id' | 'name' | 'host' | 'port' | 'type'>
export type ConfiguredTopologyLink = { from: string; to: string; kind: 'proxy' | 'jump' }

export function isTopologyProxyNode(id: string) {
  return id.startsWith(PROXY_NODE_PREFIX)
}

export function topologyProxyAddress({ host, port }: TopologyProxyNode) {
  return `${host.includes(':') && !host.startsWith('[') ? `[${host}]` : host}:${port}`
}

export function configuredTopologyRoutes(profiles: ConnectionProfile[], proxies: ProxyProfile[]) {
  const proxiesById = new Map(proxies.map((proxy) => [proxy.id, proxy]))
  const proxyNodes = new Map<string, TopologyProxyNode>()
  const links = new Map<string, ConfiguredTopologyLink>()
  const addLink = (from: string, to: string, kind: ConfiguredTopologyLink['kind']) => {
    links.set(JSON.stringify([from, to, kind]), { from, to, kind })
  }

  for (const target of profiles) {
    if (target.type === 'serial') continue
    const route = target.type === 'ssh' ? resolveSshJumpRoute(target, profiles) : null
    if (route?.error) continue
    const hops = route?.jumpHosts ?? []
    const chain = [...hops, target]
    const entry = chain[0]
    const proxyProfileId = entry.proxyProfileId?.trim()
    const proxy = proxyProfileId ? proxiesById.get(proxyProfileId) : entry.proxy
    if (
      proxy &&
      (proxy.type === 'socks5' || proxy.type === 'http') &&
      typeof proxy.host === 'string' &&
      proxy.host.trim() &&
      Number.isInteger(proxy.port) &&
      proxy.port >= 1 &&
      proxy.port <= 65535
    ) {
      const { type, host, port } = proxy
      const id =
        PROXY_NODE_PREFIX + JSON.stringify(proxyProfileId ? [proxyProfileId] : [type, host.trim().toLowerCase(), port])
      const node = { id, name: '', host: host.trim(), port, type }
      node.name = ('name' in proxy ? proxy.name : '') || topologyProxyAddress(node)
      proxyNodes.set(id, node)
      addLink(id, entry.id, 'proxy')
    }
    for (let index = 1; index < chain.length; index++) {
      addLink(chain[index - 1].id, chain[index].id, 'jump')
    }
  }

  return { proxyNodes, links: [...links.values()] }
}
