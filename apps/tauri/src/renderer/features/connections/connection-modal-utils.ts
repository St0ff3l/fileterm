import type { CreateProfileInput, SshConnectionDefaults } from '@fileterm/core'

export type SshConnectionSettingKey = keyof SshConnectionDefaults

export type ConnectionFormSetter = (
  value: CreateProfileInput | ((previous: CreateProfileInput) => CreateProfileInput)
) => void

export function effectiveConnectionSetting<K extends SshConnectionSettingKey>(
  form: CreateProfileInput,
  defaults: SshConnectionDefaults,
  key: K
): SshConnectionDefaults[K] {
  const value = (form as unknown as Record<string, unknown>)[key]
  return (value ?? defaults[key]) as SshConnectionDefaults[K]
}

export function isValidFtpCertificateFingerprint(value: string): boolean {
  const normalized = value
    .trim()
    .replace(/^sha256:/i, '')
    .replace(/[\s:]/g, '')
  if (/^[0-9a-f]{64}$/i.test(normalized)) {
    return true
  }
  // A SHA-256 digest encoded as Base64 is 43 characters without padding or
  // 44 characters with the usual trailing '='.
  return /^[A-Za-z0-9+/]{43}={0,1}$/.test(normalized)
}
