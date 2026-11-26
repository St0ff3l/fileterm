import { CONNECTION_DEVICE_ROLES, type ConnectionDeviceRole, type ConnectionProfile } from '@fileterm/core'
import type { AppIconName } from '../features/common/app-icon'
import { t } from '../i18n'

export type AnyDeviceRole = ConnectionDeviceRole | 'proxy' | 'custom'

export const DEVICE_ROLE_ICONS: Record<AnyDeviceRole, AppIconName> = {
  server: 'server',
  vm: 'vm',
  workstation: 'workstation',
  router: 'connections',
  switch: 'connections',
  firewall: 'shield',
  database: 'database',
  storage: 'disk',
  proxy: 'cloud',
  custom: 'grid'
}

export function deviceRoleLabel(role: AnyDeviceRole): string {
  switch (role) {
    case 'vm':
      return t.serverTopologyNodeRoleVm
    case 'workstation':
      return t.serverTopologyNodeRoleWorkstation
    case 'firewall':
      return t.serverTopologyNodeRoleFirewall
    case 'router':
      return t.serverTopologyNodeRoleRouter
    case 'switch':
      return t.serverTopologyNodeRoleSwitch
    case 'database':
      return t.serverTopologyNodeRoleDatabase
    case 'storage':
      return t.serverTopologyNodeRoleStorage
    case 'proxy':
      return t.serverTopologyNodeRoleProxy
    case 'custom':
      return t.serverTopologyNodeRoleCustom
    default:
      return t.serverTopologyNodeRoleServer
  }
}

export function deviceRoleIcon(role?: AnyDeviceRole): AppIconName {
  return DEVICE_ROLE_ICONS[role ?? 'server'] ?? 'server'
}

export function resolveProfileDeviceRole(
  profile?: Pick<ConnectionProfile, 'deviceRole'> | null,
  fallback: ConnectionDeviceRole = 'server'
): ConnectionDeviceRole {
  return profile?.deviceRole ?? fallback
}

export function connectionDeviceRoleOptions(): Array<{ value: ConnectionDeviceRole; label: string }> {
  return CONNECTION_DEVICE_ROLES.map((role) => ({ value: role, label: deviceRoleLabel(role) }))
}

export function collectKnownConnectionTags(profiles: ReadonlyArray<Pick<ConnectionProfile, 'tags'>>): string[] {
  return [
    ...new Set(
      profiles
        .flatMap((profile) => profile.tags ?? [])
        .map((tag) => tag.trim())
        .filter(Boolean)
    )
  ].sort((left, right) => left.localeCompare(right))
}
