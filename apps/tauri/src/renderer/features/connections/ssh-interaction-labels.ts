import type { SshAuthenticationTarget } from '@fileterm/core'
import { t } from '../../i18n'

export function sshAuthenticationTargetLabel(target: SshAuthenticationTarget) {
  switch (target) {
    case 'jump-host':
      return t.sshAuthenticationTargetJumpHost
    case 'target':
      return t.sshAuthenticationTargetTarget
    default:
      return t.sshAuthenticationTargetDirect
  }
}

export function sshInteractionConnectionLabel(target: SshAuthenticationTarget, connectionName: string) {
  const name = connectionName.trim()
  return name ? `${sshAuthenticationTargetLabel(target)} · ${name}` : sshAuthenticationTargetLabel(target)
}
