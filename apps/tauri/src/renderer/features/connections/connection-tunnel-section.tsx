import { useMemo, useState } from 'react'
import type { CreateProfileInput, SshForwardRule, TunnelProfile } from '@fileterm/core'
import { t } from '../../i18n'
import { useTunnelLibrary } from '../../hooks/use-tunnel-library'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'
import { TunnelEditDialog } from '../tunnels/tunnel-edit-dialog'
import type { ConnectionFormSetter } from './connection-modal-utils'
import { TunnelRuleEditor } from './tunnel-rule-editor'

function createConnectionForwardRule(): SshForwardRule {
  return {
    id: globalThis.crypto.randomUUID(),
    kind: 'local',
    bindHost: '127.0.0.1',
    bindPort: 8080,
    targetHost: '127.0.0.1',
    targetPort: 22,
    autoStart: true
  }
}

export function ConnectionTunnelSection({
  form,
  setForm
}: {
  form: CreateProfileInput
  setForm: ConnectionFormSetter
}) {
  const { tunnels, saveTunnel } = useTunnelLibrary()
  const sshTunnels = useMemo(() => tunnels.filter((tunnel) => tunnel.type === 'ssh'), [tunnels])
  const httpTunnels = useMemo(() => tunnels.filter((tunnel) => tunnel.type === 'http'), [tunnels])
  const selectedTunnel = useMemo(
    () => (form.tunnelProfileId ? (sshTunnels.find((tunnel) => tunnel.id === form.tunnelProfileId) ?? null) : null),
    [form.tunnelProfileId, sshTunnels]
  )
  const [isEditingTunnel, setIsEditingTunnel] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const selectValue = form.tunnelProfileId
    ? `saved:${form.tunnelProfileId}`
    : form.forwards && form.forwards.length > 0
      ? 'custom'
      : 'none'
  const options = useMemo(() => {
    const savedOption =
      form.tunnelProfileId && !selectedTunnel
        ? [{ value: `saved:${form.tunnelProfileId}`, label: t.tunnelSourceSaved }]
        : []
    return [
      { value: 'none', label: t.tunnelSourceDirect },
      ...savedOption,
      ...sshTunnels.map((tunnel) => ({
        value: `saved:${tunnel.id}`,
        label: `${tunnel.name} (${tunnel.forwards?.length ?? 0} ${t.tunnelForwardRules})`
      })),
      { value: 'custom', label: t.tunnelSourceCustom }
    ]
  }, [form.tunnelProfileId, selectedTunnel, sshTunnels])

  const handleSelectChange = (value: string) => {
    if (value === 'none') {
      setForm((previous) => ({ ...previous, tunnelProfileId: undefined, forwards: [] }))
      return
    }
    if (value === 'custom') {
      setForm((previous) => ({
        ...previous,
        tunnelProfileId: undefined,
        forwards: previous.forwards?.length ? previous.forwards : [createConnectionForwardRule()]
      }))
      return
    }
    if (value.startsWith('saved:')) {
      const tunnelId = value.slice(6)
      const tunnel = sshTunnels.find((candidate) => candidate.id === tunnelId)
      if (tunnel) {
        setForm((previous) => ({
          ...previous,
          tunnelProfileId: tunnel.id,
          forwards: tunnel.forwards?.map((rule) => ({ ...rule })) ?? []
        }))
      }
    }
  }

  const applySavedTunnel = (tunnel: TunnelProfile) => {
    setForm((previous) => ({
      ...previous,
      tunnelProfileId: tunnel.id,
      forwards: tunnel.forwards?.map((rule) => ({ ...rule })) ?? []
    }))
  }

  const handleTunnelSave = async (input: Parameters<typeof saveTunnel>[0]) => {
    setIsSubmitting(true)
    try {
      const saved = await saveTunnel(input)
      if (saved.type === 'ssh') applySavedTunnel(saved)
      setIsEditingTunnel(false)
    } finally {
      setIsSubmitting(false)
    }
  }

  const updateRule = (rule: SshForwardRule, patch: Partial<SshForwardRule>) => {
    setForm((previous) => ({
      ...previous,
      forwards: previous.forwards?.map((item) => (item.id === rule.id ? { ...item, ...patch } : item))
    }))
  }

  return (
    <fieldset className="ssh-fieldset tunnel-fieldset">
      <legend className="proxy-section-legend">
        <span>{t.tunnel}</span>
      </legend>
      <div className="tunnel-intro">
        <AppIcon name="info" size={15} className="tunnel-intro__icon" />
        <p>
          <span>{t.tunnelAutoStartHint}</span>
          <span className="tunnel-independent-note">{t.networkProfilesLocationHint}</span>
        </p>
      </div>

      <div className="tunnel-source-row">
        <label>
          {t.tunnelSource}:
          <DropdownSelect value={selectValue} options={options} onChange={handleSelectChange} />
        </label>
      </div>
      {httpTunnels.length > 0 ? (
        <div className="tunnel-profile-note tunnel-profile-note--connection">
          <AppIcon name="info" size={15} />
          <span>{t.httpTunnelConnectionUnavailable}</span>
        </div>
      ) : null}

      {selectedTunnel ? (
        <div className="proxy-saved-selected-card tunnel-saved-selected-card">
          <div className="proxy-saved-selected-header">
            <div className="proxy-saved-selected-info">
              <span className="proxy-type-badge proxy-type-badge--socks5">SSH</span>
              <strong className="proxy-saved-selected-name">{selectedTunnel.name}</strong>
              <span className="proxy-saved-selected-endpoint">
                {selectedTunnel.forwards?.length ?? 0} {t.tunnelForwardRules}
              </span>
            </div>
            <button
              className="flat-button proxy-card-edit-btn"
              title={t.edit}
              type="button"
              onClick={() => setIsEditingTunnel(true)}
            >
              <AppIcon name="edit" size={13} />
              <span>{t.edit}</span>
            </button>
          </div>
          <div className="proxy-saved-selected-note">
            <AppIcon name="info" size={14} />
            <span>{t.tunnelBoundHint}</span>
          </div>
        </div>
      ) : null}

      {selectValue === 'custom' ? (
        <>
          <div className="tunnel-rule-list">
            {(form.forwards ?? []).map((rule, index) => (
              <TunnelRuleEditor
                key={rule.id}
                index={index}
                rule={rule}
                onChange={(patch) => updateRule(rule, patch)}
                onRemove={() =>
                  setForm((previous) => ({
                    ...previous,
                    forwards: previous.forwards?.filter((item) => item.id !== rule.id)
                  }))
                }
              />
            ))}
          </div>
          <button
            className="tunnel-add-button"
            type="button"
            onClick={() =>
              setForm((previous) => ({
                ...previous,
                forwards: [...(previous.forwards ?? []), createConnectionForwardRule()]
              }))
            }
          >
            <AppIcon name="plus" size={14} />
            {t.addConnectionTunnel}
          </button>
        </>
      ) : null}

      {selectedTunnel ? (
        <TunnelEditDialog
          initialTunnel={selectedTunnel}
          isOpen={isEditingTunnel}
          isSubmitting={isSubmitting}
          onClose={() => setIsEditingTunnel(false)}
          onSave={handleTunnelSave}
        />
      ) : null}
    </fieldset>
  )
}
