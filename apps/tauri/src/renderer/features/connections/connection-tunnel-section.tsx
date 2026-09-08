import { useEffect, useMemo, useRef, useState } from 'react'
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
  const [isCreatingTunnel, setIsCreatingTunnel] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const isWaitingForNewTunnelRef = useRef(false)
  const previousTunnelIdsRef = useRef<Set<string>>(new Set(sshTunnels.map((t) => t.id)))

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

  useEffect(() => {
    if (isWaitingForNewTunnelRef.current) {
      const newTunnel = sshTunnels.find((t) => !previousTunnelIdsRef.current.has(t.id))
      if (newTunnel) {
        handleSelectChange(`saved:${newTunnel.id}`)
        isWaitingForNewTunnelRef.current = false
      }
    }
    previousTunnelIdsRef.current = new Set(sshTunnels.map((t) => t.id))
  }, [sshTunnels])

  useEffect(() => {
    if (selectedTunnel && form.tunnelProfileId === selectedTunnel.id) {
      setForm((prev) => ({
        ...prev,
        forwards: selectedTunnel.forwards?.map((rule) => ({ ...rule })) ?? []
      }))
    }
  }, [selectedTunnel, form.tunnelProfileId, setForm])

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
      setIsCreatingTunnel(false)
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleCreateTunnel = () => {
    isWaitingForNewTunnelRef.current = true
    if (window.fileterm?.openTunnelFormWindow) {
      void window.fileterm.openTunnelFormWindow('create', undefined, 'ssh')
    } else {
      setIsCreatingTunnel(true)
    }
  }

  const handleEditTunnel = () => {
    if (!selectedTunnel) return
    if (window.fileterm?.openTunnelFormWindow) {
      void window.fileterm.openTunnelFormWindow('edit', selectedTunnel.id)
    } else {
      setIsEditingTunnel(true)
    }
  }

  const updateRule = (rule: SshForwardRule, patch: Partial<SshForwardRule>) => {
    setForm((previous) => ({
      ...previous,
      forwards: previous.forwards?.map((item) => (item.id === rule.id ? { ...item, ...patch } : item))
    }))
  }

  return (
    <fieldset className="ssh-fieldset">
      <legend>{t.tunnel}</legend>

      <div className="network-card__body">
        <div className="network-setting-row">
          <span className="network-setting-row__label">{t.tunnelSource}</span>
          <div className="network-setting-row__control">
            <DropdownSelect
              className="network-select-control"
              value={selectValue}
              options={options}
              onChange={handleSelectChange}
            />
            <button
              type="button"
              className="flat-button network-create-btn"
              onClick={handleCreateTunnel}
              title={t.newSshTunnel}
            >
              <AppIcon name="plus" size={13} />
              <span>{t.newSshTunnel}</span>
            </button>
          </div>
        </div>

        <p className="network-field-hint">{t.tunnelAutoStartHint}</p>

        {httpTunnels.length > 0 ? (
          <p className="network-field-hint network-field-hint--subtle">
            <AppIcon name="info" size={13} className="network-field-hint__icon" />
            <span>{t.httpTunnelConnectionUnavailable}</span>
          </p>
        ) : null}

        {selectedTunnel ? (
          <div className="proxy-saved-selected-card tunnel-saved-selected-card">
            <div className="proxy-saved-selected-header">
              <div className="proxy-saved-selected-info">
                <span
                  className={`proxy-type-badge proxy-type-badge--${selectedTunnel.type === 'ssh' ? 'socks5' : 'http'}`}
                >
                  SSH
                </span>
                <strong className="proxy-saved-selected-name">{selectedTunnel.name}</strong>
                <span className="proxy-saved-selected-endpoint">
                  {selectedTunnel.forwards?.length ?? 0} {t.tunnelForwardRules}
                </span>
              </div>
              <button
                className="flat-button proxy-card-edit-btn"
                title={t.edit}
                type="button"
                onClick={handleEditTunnel}
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
          <div className="tunnel-custom-area">
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
          </div>
        ) : null}
      </div>

      {selectedTunnel && isEditingTunnel ? (
        <TunnelEditDialog
          initialTunnel={selectedTunnel}
          isOpen={isEditingTunnel}
          isSubmitting={isSubmitting}
          onClose={() => setIsEditingTunnel(false)}
          onSave={handleTunnelSave}
        />
      ) : null}

      {isCreatingTunnel ? (
        <TunnelEditDialog
          isOpen={isCreatingTunnel}
          initialType="ssh"
          isSubmitting={isSubmitting}
          onClose={() => setIsCreatingTunnel(false)}
          onSave={handleTunnelSave}
        />
      ) : null}
    </fieldset>
  )
}
