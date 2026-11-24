import type { ConnectionProfile, CreateProfileInput } from '@fileterm/core'
import { validateConnectionHost } from '@fileterm/shared'

export function outboundAddressFamily(host: string): 'ipv4' | 'ipv6' | null {
  const address = validateConnectionHost(host)
  if (!address.valid) return null
  const ip = address.normalizedHost
  if (address.kind === 'ipv4' && /^(0|[1-9]\d{0,2})(\.(0|[1-9]\d{0,2})){3}$/.test(ip)) {
    const firstOctet = Number(ip.split('.')[0])
    return firstOctet === 127 ||
      (firstOctet >= 224 && firstOctet <= 239) ||
      ip === '0.0.0.0' ||
      ip === '255.255.255.255'
      ? null
      : 'ipv4'
  }
  if (address.kind !== 'ipv6' || ip.includes('%')) return null
  const canonical = new URL(`http://[${ip}]`).hostname.slice(1, -1)
  return /^(::(?:1$|$|ffff:)|fe[89ab]|ff)/i.test(canonical) ? null : 'ipv6'
}

export function outboundInterfaceConflict(form: CreateProfileInput, usedAsJumpHost: boolean) {
  if (form.type !== 'ssh' || form.authType === 'sshConfig') return 'profile'
  if (form.proxyProfileId?.trim() || (form.proxy?.type && form.proxy.type !== 'none')) return 'proxy'
  if (usedAsJumpHost || (form.jumpProfileIds?.length ?? Number(form.jumpProfileId != null))) return 'jump'
  return outboundAddressFamily(form.host) ? null : 'address'
}

export function isUsedAsJumpHost(id: string | null, profiles: ConnectionProfile[]) {
  return Boolean(
    id &&
    profiles.some(
      (profile) =>
        (profile.type === 'ssh' || profile.type === 'sftp') &&
        (profile.jumpProfileIds ? profile.jumpProfileIds.includes(id) : profile.jumpProfileId === id)
    )
  )
}
