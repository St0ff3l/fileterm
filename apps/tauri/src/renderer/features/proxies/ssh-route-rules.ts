import type { ConnectionProfile, SshProfile, SshRouteCandidate, SshRoutePolicy, SshRouteRule } from '@fileterm/core'

export function eligibleSshRoutes(profiles: ConnectionProfile[]): SshProfile[] {
  return profiles.filter(
    (profile): profile is SshProfile =>
      profile.type === 'ssh' &&
      profile.authType !== 'sshConfig' &&
      !(profile.jumpProfileIds ? profile.jumpProfileIds.length : profile.jumpProfileId)
  )
}

export function sshRouteKey(route: SshRouteCandidate): string {
  if (route.type === 'proxy') return `proxy:${route.proxyId}`
  if (route.type === 'interface') return `interface:${route.adapterId}`
  return 'direct'
}

export function sshRuleRoutes(rule: SshRouteRule | null): SshRouteCandidate[] {
  if (!rule) return []
  if (rule.mode === 'fixed') return [rule.route]
  if (rule.mode === 'preferred') return [rule.primary, ...rule.fallback]
  return rule.routes
}

export function effectiveSshRule(profile: SshProfile, policy: SshRoutePolicy) {
  if (Object.hasOwn(policy.servers, profile.id)) {
    return { scope: 'server', label: profile.name, rule: policy.servers[profile.id] } as const
  }
  if (profile.sshNetworkInterface || profile.proxyProfileId || (profile.proxy && profile.proxy.type !== 'none')) {
    return { scope: 'legacy', rule: null } as const
  }
  const matching = (profile.tags ?? [])
    .filter((tag) => Object.hasOwn(policy.tags, tag))
    .map((tag) => ({ ...policy.tags[tag], tag }))
    .sort((left, right) => right.priority - left.priority || (left.tag < right.tag ? -1 : 1))
  const [top] = matching
  if (!top) return { scope: 'default', rule: policy.default } as const
  const fingerprint = (rule: SshRouteRule) => JSON.stringify([rule.mode, sshRuleRoutes(rule).map(sshRouteKey)])
  if (matching.some(({ priority, rule }) => priority === top.priority && fingerprint(rule) !== fingerprint(top.rule))) {
    return { scope: 'conflict', rule: null } as const
  }
  return { scope: 'tag', label: top.tag, rule: top.rule } as const
}

export function fallbackSshRuleSource(
  profile: {
    tags?: SshProfile['tags']
    sshNetworkInterface?: SshProfile['sshNetworkInterface'] | null
    proxyProfileId?: SshProfile['proxyProfileId']
    proxy?: SshProfile['proxy']
  },
  policy?: SshRoutePolicy
) {
  if (profile.sshNetworkInterface || profile.proxyProfileId || (profile.proxy && profile.proxy.type !== 'none')) {
    return { kind: 'manual' } as const
  }
  if (!policy) return { kind: 'direct' } as const
  const matching = (profile.tags ?? [])
    .filter((tag) => Object.hasOwn(policy.tags, tag))
    .map((tag) => ({ ...policy.tags[tag], tag }))
    .sort((left, right) => right.priority - left.priority || (left.tag < right.tag ? -1 : 1))
  const [top] = matching
  if (top) {
    const fingerprint = (rule: SshRouteRule) => JSON.stringify([rule.mode, sshRuleRoutes(rule).map(sshRouteKey)])
    if (
      matching.some(({ priority, rule }) => priority === top.priority && fingerprint(rule) !== fingerprint(top.rule))
    ) {
      return { kind: 'conflict' } as const
    }
    return { kind: 'tag', tag: top.tag } as const
  }
  return policy.default ? ({ kind: 'global' } as const) : ({ kind: 'direct' } as const)
}

export type SshRouteEdit =
  | { scope: 'default'; rule: SshRouteRule | null }
  | { scope: 'tag'; tag: string; priority: number; rule: SshRouteRule | null }
  | { scope: 'server'; profileId: string; rule: SshRouteRule | null }
  | { scope: 'batch'; tag: string; rule: SshRouteRule }

export function applySshRouteEdit(
  baseline: SshRoutePolicy,
  edit: SshRouteEdit,
  profiles: ConnectionProfile[]
): SshRoutePolicy {
  if (edit.scope === 'default') return { ...baseline, default: edit.rule }
  if (edit.scope === 'tag') {
    const tags = { ...baseline.tags }
    if (edit.rule) {
      return { ...baseline, tags: { ...tags, [edit.tag]: { priority: edit.priority, rule: edit.rule } } }
    }
    delete tags[edit.tag]
    return { ...baseline, tags }
  }
  if (edit.scope === 'batch') {
    const targets = eligibleSshRoutes(profiles).filter((profile) => profile.tags?.includes(edit.tag))
    return {
      ...baseline,
      servers: { ...baseline.servers, ...Object.fromEntries(targets.map((profile) => [profile.id, edit.rule])) }
    }
  }
  const servers = { ...baseline.servers }
  if (edit.rule) return { ...baseline, servers: { ...servers, [edit.profileId]: edit.rule } }
  delete servers[edit.profileId]
  return { ...baseline, servers }
}
