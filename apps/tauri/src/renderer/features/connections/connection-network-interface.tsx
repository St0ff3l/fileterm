import type { CreateProfileInput, SshNetworkInterfaceInfo } from '@fileterm/core'
import { useEffect, useState } from 'react'
import { outboundAddressFamily, outboundInterfaceConflict } from '../../app/ssh-network-interface'
import { Button } from '../../components/common/button/button'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionNetworkInterface({
  form,
  setForm,
  usedAsJumpHost
}: {
  form: CreateProfileInput
  setForm: ConnectionFormSetter
  usedAsJumpHost: boolean
}) {
  const onWindows = window.fileterm?.platform === 'win32'
  const [adapters, setAdapters] = useState<SshNetworkInterfaceInfo[]>([])
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading')
  const [refresh, setRefresh] = useState(0)
  const requirement = outboundInterfaceConflict(form, usedAsJumpHost)
  const family = outboundAddressFamily(form.host)
  const preference = form.sshNetworkInterface

  useEffect(() => {
    if (!onWindows) return
    let current = true
    const api = window.fileterm
    void Promise.resolve()
      .then(() => api?.listSshNetworkInterfaces())
      .then((interfaces) => {
        if (!current) return
        if (interfaces) {
          setAdapters(interfaces)
          setStatus('ready')
        } else {
          setStatus('error')
        }
      })
      .catch(() => {
        if (current) setStatus('error')
      })
    return () => {
      current = false
    }
  }, [onWindows, refresh])

  if ((!onWindows || form.type !== 'ssh') && !preference) return null
  const selected = adapters.find((adapter) => adapter.id === preference?.adapterId)
  const available = (adapter: SshNetworkInterfaceInfo) =>
    Boolean(adapter.online && family && adapter[family] && !requirement && status === 'ready')
  const hint = !onWindows
    ? t.sshInterfaceWindowsOnly
    : requirement
      ? {
          profile: t.sshInterfaceProfileConflict,
          proxy: t.sshInterfaceProxyConflict,
          jump: t.sshInterfaceJumpConflict,
          address: t.sshInterfaceAddressRequired
        }[requirement]
      : status === 'error'
        ? t.sshInterfaceLoadFailed
        : status === 'ready' && preference && (!selected || !available(selected))
          ? t.sshInterfaceUnavailable
          : null
  const options = [
    { value: '', label: t.sshInterfaceSystemDefault },
    ...(preference && !selected
      ? [{ value: preference.adapterId, label: preference.displayName || preference.adapterId, disabled: true }]
      : []),
    ...adapters.map((adapter) => ({
      value: adapter.id,
      label: adapters.some((other) => other.id !== adapter.id && other.name === adapter.name)
        ? `${adapter.name} (${adapter.id.slice(0, 8)})`
        : adapter.name,
      disabled: !available(adapter)
    }))
  ]

  return (
    <fieldset className="ssh-fieldset network-interface-fieldset">
      <legend>{t.sshOutboundInterface}</legend>
      <div className="network-card__body">
        <div className="network-setting-row__control">
          <DropdownSelect
            id="ssh-outbound-interface"
            ariaLabel={t.sshOutboundInterface}
            className="network-select-control network-routing-select"
            options={options}
            value={preference?.adapterId ?? ''}
            onChange={(id) => {
              const adapter = adapters.find((item) => item.id === id)
              if (id && (!adapter || !available(adapter))) return
              setForm((prev) => ({
                ...prev,
                sshNetworkInterface: adapter ? { adapterId: adapter.id, displayName: adapter.name } : null
              }))
            }}
          />
          {onWindows ? (
            <Button
              aria-busy={status === 'loading'}
              className="network-interface-refresh"
              type="button"
              disabled={status === 'loading'}
              icon={<AppIcon name="refresh" size={14} />}
              size="md"
              onClick={() => {
                setStatus('loading')
                setRefresh((value) => value + 1)
              }}
            >
              {status === 'loading' ? t.refreshingList : t.refreshList}
            </Button>
          ) : null}
        </div>
        {hint ? (
          <p className="network-field-hint network-field-hint--warning" role="status">
            {hint}
          </p>
        ) : null}
      </div>
    </fieldset>
  )
}
