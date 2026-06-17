export type ConnectionHostKind = 'hostname' | 'ipv4' | 'ipv6'

export type ConnectionHostValidationResult =
  | {
      valid: true
      normalizedHost: string
      kind: ConnectionHostKind
    }
  | {
      valid: false
      normalizedHost: string
      reason: 'empty' | 'invalid-format'
    }

const INVALID_HOST_CHARACTERS = /[\s/@?#\\]/u
const URL_SCHEME_PREFIX = /^[a-z][a-z\d+.-]*:\/\//iu
const IPV4_SEGMENT = /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)$/
const IPV6_ZONE = /^[A-Za-z0-9_.-]+$/

export function normalizeConnectionHost(rawHost: string): string {
  const trimmed = rawHost.trim()
  if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
    return trimmed.slice(1, -1).trim()
  }
  return trimmed
}

export function validateConnectionHost(rawHost: string): ConnectionHostValidationResult {
  const normalizedHost = normalizeConnectionHost(rawHost)
  if (!normalizedHost) {
    return {
      valid: false,
      normalizedHost,
      reason: 'empty'
    }
  }

  const trimmed = rawHost.trim()
  if (
    INVALID_HOST_CHARACTERS.test(trimmed) ||
    URL_SCHEME_PREFIX.test(trimmed) ||
    hasMismatchedIpv6Brackets(trimmed)
  ) {
    return {
      valid: false,
      normalizedHost,
      reason: 'invalid-format'
    }
  }

  if (normalizedHost.includes(':')) {
    if (!isValidIpv6Host(normalizedHost)) {
      return {
        valid: false,
        normalizedHost,
        reason: 'invalid-format'
      }
    }

    return {
      valid: true,
      normalizedHost,
      kind: 'ipv6'
    }
  }

  if (looksLikeIpv4(normalizedHost)) {
    if (!isValidIpv4Host(normalizedHost)) {
      return {
        valid: false,
        normalizedHost,
        reason: 'invalid-format'
      }
    }

    return {
      valid: true,
      normalizedHost,
      kind: 'ipv4'
    }
  }

  if (/[()[\]{}]/u.test(normalizedHost)) {
    return {
      valid: false,
      normalizedHost,
      reason: 'invalid-format'
    }
  }

  return {
    valid: true,
    normalizedHost,
    kind: 'hostname'
  }
}

function hasMismatchedIpv6Brackets(host: string): boolean {
  if (host.startsWith('[') || host.endsWith(']')) {
    return !(host.startsWith('[') && host.endsWith(']'))
  }
  return false
}

function looksLikeIpv4(host: string): boolean {
  return /^[\d.]+$/.test(host)
}

function isValidIpv4Host(host: string): boolean {
  const segments = host.split('.')
  return segments.length === 4 && segments.every((segment) => IPV4_SEGMENT.test(segment))
}

function isValidIpv6Host(host: string): boolean {
  const zoneStart = host.indexOf('%')
  const address = zoneStart === -1 ? host : host.slice(0, zoneStart)
  const zone = zoneStart === -1 ? '' : host.slice(zoneStart + 1)

  if (!address || (zoneStart !== -1 && (!zone || !IPV6_ZONE.test(zone) || host.indexOf('%', zoneStart + 1) !== -1))) {
    return false
  }

  try {
    const escaped = zone ? `${address}%25${zone}` : address
    new URL(`ssh://[${escaped}]`)
    return true
  } catch {
    return false
  }
}
