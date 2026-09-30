import type { CreateProfileInput, SshHostVerificationRequest } from '@fileterm/core'
import { normalizeConnectionHost } from '@fileterm/shared'

function sshEndpointMatches(form: CreateProfileInput, host: string, port: number): boolean {
  return (
    form.type === 'ssh' &&
    normalizeConnectionHost(form.host) === normalizeConnectionHost(host) &&
    (Number(form.port) || 22) === port
  )
}

/** Carry an acknowledged test decision into the form that will be saved.
 * A new profile has no durable id yet, so the SSH worker cannot save its key.
 */
export function acceptConnectionTestHostTrust(
  form: CreateProfileInput,
  request: SshHostVerificationRequest,
  editingProfileId: string | null
): CreateProfileInput {
  if (
    !request.tabId.startsWith('connection-test-') ||
    request.authenticationTarget === 'jump-host' ||
    request.profileId !== (editingProfileId ?? '') ||
    !sshEndpointMatches(form, request.host, request.port)
  )
    return form

  return { ...form, trustedHostFingerprint: request.fingerprint }
}

/** Editing the destination must not carry a previously accepted key with it. */
export function invalidateChangedConnectionHostTrust(
  previous: CreateProfileInput,
  next: CreateProfileInput
): CreateProfileInput {
  if (
    previous.type === 'ssh' &&
    previous.trustedHostFingerprint &&
    next.trustedHostFingerprint === previous.trustedHostFingerprint &&
    (!sshEndpointMatches(next, previous.host, Number(previous.port) || 22) ||
      next.jumpProfileId !== previous.jumpProfileId)
  )
    return { ...next, trustedHostFingerprint: '' }

  return next
}
