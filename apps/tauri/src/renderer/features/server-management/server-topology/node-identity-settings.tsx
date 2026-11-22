import { useId } from 'react'
import { Button } from '../../../components/common/button/button'
import { DropdownSelect } from '../../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../../components/common/input/input'
import { t } from '../../../i18n'
import { TOPOLOGY_NODE_ROLES, TOPOLOGY_NODE_LABEL_LIMIT, type TopologyNodeRole } from '../server-topology-model'

const roleLabels: Record<TopologyNodeRole, string> = {
  server: t.serverTopologyNodeRoleServer,
  firewall: t.serverTopologyNodeRoleFirewall,
  router: t.serverTopologyNodeRoleRouter,
  switch: t.serverTopologyNodeRoleSwitch,
  database: t.serverTopologyNodeRoleDatabase,
  storage: t.serverTopologyNodeRoleStorage,
  proxy: t.serverTopologyNodeRoleProxy,
  custom: t.serverTopologyNodeRoleCustom
}

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

  return (
    <section className="server-topology__identity-settings">
      <label htmlFor={`${id}-role`}>{t.serverTopologyNodeRole}</label>
      <DropdownSelect
        id={`${id}-role`}
        ariaLabel={t.serverTopologyNodeRole}
        onChange={(value) => onChange(value as TopologyNodeRole, label)}
        options={TOPOLOGY_NODE_ROLES.map((value) => ({ value, label: roleLabels[value] }))}
        value={role}
      />
      <div className="server-topology__identity-label">
        <label htmlFor={`${id}-label`}>{t.serverTopologyNodeLabel}</label>
        <Button disabled={!label} onClick={() => onChange(role, '')} size="panel" variant="ghost">
          {t.serverTopologyNodeLabelRestore}
        </Button>
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
