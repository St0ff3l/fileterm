import { useId } from 'react'
import { CONNECTION_DEVICE_ROLES, type ConnectionDeviceRole } from '@fileterm/core'
import { deviceRoleLabel } from '../../../app/device-role'
import { DropdownSelect } from '../../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../../components/common/input/input'
import { PanelActionButton } from '../../../components/common/panel-action-button/panel-action-button'
import { t } from '../../../i18n'
import { TOPOLOGY_NODE_LABEL_LIMIT, type TopologyNodeRole } from '../server-topology-model'

export function NodeIdentitySettings({
  role,
  label,
  onChange
}: {
  role: TopologyNodeRole
  label: string
  onChange(role: TopologyNodeRole, label: string): void
}) {
  const id = useId()
  const baseRoles: readonly TopologyNodeRole[] = CONNECTION_DEVICE_ROLES
  const options = (baseRoles.includes(role as ConnectionDeviceRole) ? baseRoles : [...baseRoles, role]).map(
    (value) => ({ value, label: deviceRoleLabel(value) })
  )

  return (
    <section className="server-topology__identity-settings">
      <label htmlFor={`${id}-role`}>{t.serverTopologyNodeRole}</label>
      <DropdownSelect
        id={`${id}-role`}
        ariaLabel={t.serverTopologyNodeRole}
        onChange={(value) => onChange(value as TopologyNodeRole, label)}
        options={options}
        value={role}
      />
      <div className="server-topology__identity-label">
        <label htmlFor={`${id}-label`}>{t.serverTopologyNodeLabel}</label>
        <PanelActionButton disabled={!label} onClick={() => onChange(role, '')}>
          {t.serverTopologyNodeLabelRestore}
        </PanelActionButton>
      </div>
      <Input
        id={`${id}-label`}
        maxLength={TOPOLOGY_NODE_LABEL_LIMIT}
        onChange={(event) => onChange(role, event.target.value)}
        placeholder={t.serverTopologyNodeLabelPlaceholder}
        value={label}
      />
    </section>
  )
}
