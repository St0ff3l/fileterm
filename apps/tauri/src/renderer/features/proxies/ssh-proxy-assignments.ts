import type { ConnectionProfile, SshProfile, SshProxyAssignmentInput, SshProxySource } from '@fileterm/core'

export function sshProxySource(profile: SshProfile): SshProxySource {
  if (profile.proxyProfileId?.trim()) return { kind: 'saved', proxyId: profile.proxyProfileId }
  if (!profile.proxy?.type || profile.proxy.type === 'none') return { kind: 'direct' }
  const { type, host, port, username } = profile.proxy
  return { kind: 'custom', type, host, port, username: username || undefined }
}

export function proxyAssignmentRows(profiles: ConnectionProfile[], proxyId: string) {
  const laterHops = new Set(
    profiles.flatMap((profile) =>
      (profile.type === 'ssh' || profile.type === 'sftp') && Array.isArray(profile.jumpProfileIds)
        ? profile.jumpProfileIds.slice(1)
        : []
    )
  )
  return profiles.flatMap((profile) => {
    if (profile.type !== 'ssh' || profile.authType === 'sshConfig') return []
    const source = sshProxySource(profile)
    const assigned = source.kind === 'saved' && source.proxyId === proxyId
    const hasOwnJump =
      profile.jumpProfileIds !== undefined
        ? !Array.isArray(profile.jumpProfileIds) || profile.jumpProfileIds.length > 0
        : profile.jumpProfileId != null
    const blocked = hasOwnJump || (!assigned && laterHops.has(profile.id))
    return [{ profile, source, assigned, blocked }]
  })
}

export function proxyAssignmentChanges(
  rows: ReturnType<typeof proxyAssignmentRows>,
  edits: ReadonlyMap<string, boolean>
): SshProxyAssignmentInput['changes'] {
  return rows.flatMap(({ profile, source, assigned, blocked }) => {
    const next = edits.get(profile.id)
    return !blocked && next !== undefined && assigned !== next
      ? [{ profileId: profile.id, assigned: next, expected: source }]
      : []
  })
}
