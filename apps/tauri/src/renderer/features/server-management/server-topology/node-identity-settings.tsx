import { useEffect, useId, useState } from 'react'
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
  onSave
}: {
  role: TopologyNodeRole
  label: string
  onSave(role: TopologyNodeRole, label: string): void
}) {
  const id = useId()
  const [draftRole, setDraftRole] = useState(role)
  const [draftLabel, setDraftLabel] = useState(label)

  useEffect(() => {
    setDraftRole(role)
    setDraftLabel(label)
  }, [role, label])

  const changed = draftRole !== role || draftLabel.trim() !== label

  return (
    <section className="server-topology__identity-settings">
      <h3>{t.serverTopologyNodeRole}</h3>
      <label htmlFor={`${id}-role`}>{t.serverTopologyNodeRole}</label>
      <DropdownSelect
        id={`${id}-role`}
        ariaLabel={t.serverTopologyNodeRole}
        onChange={(value) => setDraftRole(value as TopologyNodeRole)}
        options={TOPOLOGY_NODE_ROLES.map((value) => ({ value, label: roleLabels[value] }))}
        value={draftRole}
      />
      <label htmlFor={`${id}-label`}>{t.serverTopologyNodeLabel}</label>
      <Input
        id={`${id}-label`}
        maxLength={TOPOLOGY_NODE_LABEL_LIMIT}
        onChange={(event) => setDraftLabel(event.target.value)}
        placeholder={t.serverTopologyNodeLabelPlaceholder}
        value={draftLabel}
      />
      <div className="server-topology__actions">
        <Button disabled={!changed} onClick={() => onSave(draftRole, draftLabel)} size="panel">
          {t.save}
        </Button>
      </div>
    </section>
  )
}
