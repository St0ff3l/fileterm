import type { CreateProfileInput, SshProfile } from '@fileterm/core'
import { SegmentedControl } from '../../components/common/segmented-control/segmented-control'
import { t } from '../../i18n'
import './ssh-jump-route.css'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'
import { ConnectionProxySection } from './connection-proxy-section'
import { ConnectionTunnelSection } from './connection-tunnel-section'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionNetworkSection({
  form,
  jumpHosts,
  jumpRoute,
  routeError,
  routingMode,
  setForm
}: {
  form: CreateProfileInput
  jumpHosts: SshProfile[]
  jumpRoute: SshProfile[]
  routeError: string | null
  routingMode: 'direct' | 'jump'
  setForm: ConnectionFormSetter
}) {
  const supportsRouting = form.type === 'ssh'
  const supportsProxy = form.type === 'ssh' || form.type === 'sftp' || form.type === 'telnet' || form.type === 'ftp'
  const supportsTunnel = form.type === 'ssh'
  const selectedJumpProfileIds =
    form.jumpProfileIds ?? (form.jumpProfileId ? jumpRoute.map((profile) => profile.id) : [])

  const updateJumpProfileIds = (ids: string[]) =>
    setForm((prev) => ({ ...prev, jumpProfileId: undefined, jumpProfileIds: ids }))

  const jumpHostOptions = (index: number) => {
    const selectedId = selectedJumpProfileIds[index]
    return jumpHosts
      .filter((profile) => {
        if (profile.id === selectedId) return true
        if (selectedJumpProfileIds.some((id, selectedIndex) => selectedIndex !== index && id === profile.id)) {
          return false
        }
        if (profile.jumpProfileId || profile.jumpProfileIds?.length) return false
        const hasProxy =
          Boolean(profile.proxyProfileId) || Boolean(profile.proxy?.type && profile.proxy.type !== 'none')
        return index === 0 || !hasProxy
      })
      .map((profile) => ({
        value: profile.id,
        label: `${profile.name} (${profile.host}:${profile.port})`
      }))
  }
  const hasUnavailableJumpHostSlot = selectedJumpProfileIds.length
    ? selectedJumpProfileIds.some((id, index) => !id && jumpHostOptions(index).length === 0)
    : jumpHostOptions(0).length === 0

  return (
    <div className="ssh-form-page network-form-page">
      <div className="network-overview-tip">
        <AppIcon name="info" size={14} className="network-overview-tip__icon" />
        <p className="network-overview-tip__text">{t.networkProfilesLocationHint}</p>
      </div>

      {supportsRouting ? (
        <fieldset className="ssh-fieldset">
          <legend>{t.networkRouting}</legend>
          <div className="network-card__body">
            <div className="network-setting-row">
              <span className="network-setting-row__label">{t.route}</span>
              <div className="network-setting-row__control">
                <SegmentedControl<'direct' | 'jump'>
                  ariaLabel={t.route}
                  className="network-routing-modes"
                  value={routingMode}
                  options={[
                    { value: 'direct', label: t.direct },
                    { value: 'jump', label: t.viaJumpHost }
                  ]}
                  onChange={(value) => {
                    if (value === 'direct') {
                      setForm((prev) => ({ ...prev, jumpProfileId: undefined, jumpProfileIds: undefined }))
                    } else if (!selectedJumpProfileIds.length) {
                      updateJumpProfileIds([''])
                    }
                  }}
                />
              </div>
            </div>

            {routingMode === 'jump' ? (
              <div className="network-setting-subgroup">
                <div className="ssh-jump-hops">
                  {selectedJumpProfileIds.map((selectedId, index) => {
                    const options = jumpHostOptions(index)
                    return (
                      <div className="ssh-jump-hop" key={`jump-hop-${index}`}>
                        <label className="ssh-jump-hop__label" htmlFor={`connection-jump-host-${index}`}>
                          {t.jumpHost} {index + 1}
                        </label>
                        <DropdownSelect
                          id={`connection-jump-host-${index}`}
                          ariaLabel={`${t.jumpHost} ${index + 1}`}
                          className="network-select-control network-routing-select ssh-jump-hop__select"
                          placeholder={t.selectJumpHost}
                          value={selectedId}
                          options={[{ value: '', label: t.selectJumpHost, disabled: true }, ...options]}
                          onChange={(value) => {
                            const next = [...selectedJumpProfileIds]
                            next[index] = value
                            updateJumpProfileIds(next)
                          }}
                        />
                        <div className="ssh-jump-hop__actions">
                          <button
                            aria-label={`${t.moveJumpHostUp}: ${t.jumpHost} ${index + 1}`}
                            className="ssh-jump-hop__action"
                            disabled={index === 0}
                            onClick={() => {
                              const next = [...selectedJumpProfileIds]
                              ;[next[index - 1], next[index]] = [next[index], next[index - 1]]
                              updateJumpProfileIds(next)
                            }}
                            title={t.moveJumpHostUp}
                            type="button"
                          >
                            <AppIcon name="arrow-up" size={13} />
                          </button>
                          <button
                            aria-label={`${t.moveJumpHostDown}: ${t.jumpHost} ${index + 1}`}
                            className="ssh-jump-hop__action"
                            disabled={index === selectedJumpProfileIds.length - 1}
                            onClick={() => {
                              const next = [...selectedJumpProfileIds]
                              ;[next[index], next[index + 1]] = [next[index + 1], next[index]]
                              updateJumpProfileIds(next)
                            }}
                            title={t.moveJumpHostDown}
                            type="button"
                          >
                            <AppIcon name="arrow-down" size={13} />
                          </button>
                          <button
                            aria-label={`${t.removeJumpHost}: ${t.jumpHost} ${index + 1}`}
                            className="ssh-jump-hop__action ssh-jump-hop__action--remove"
                            onClick={() => updateJumpProfileIds(selectedJumpProfileIds.filter((_, i) => i !== index))}
                            title={t.removeJumpHost}
                            type="button"
                          >
                            <AppIcon name="trash" size={13} />
                          </button>
                        </div>
                      </div>
                    )
                  })}
                  {selectedJumpProfileIds.length < 8 ? (
                    <button
                      className="ssh-jump-hop__add"
                      onClick={() => updateJumpProfileIds([...selectedJumpProfileIds, ''])}
                      type="button"
                    >
                      <AppIcon name="plus" size={13} />
                      {t.addJumpHost}
                    </button>
                  ) : null}
                  <div className="ssh-jump-hops__feedback">
                    <p className="network-field-hint">{t.jumpHostHint}</p>
                    {hasUnavailableJumpHostSlot ? (
                      <p className="network-field-hint network-field-hint--warning">{t.noAvailableJumpHost}</p>
                    ) : null}
                    {routeError ? (
                      <p className="network-field-hint network-field-hint--warning" role="alert">
                        {routeError}
                      </p>
                    ) : null}
                  </div>
                </div>
              </div>
            ) : null}

            {routingMode === 'jump' && !routeError && selectedJumpProfileIds.length ? (
              <ol className="ssh-jump-route" aria-label={t.route}>
                <li>FileTerm</li>
                {jumpRoute.map((hop) => (
                  <li key={hop.id} title={`${hop.host}:${hop.port}`}>
                    {hop.name}
                  </li>
                ))}
                <li>{form.name?.trim() || form.host || t.sshJumpRouteTarget}</li>
              </ol>
            ) : null}
          </div>
        </fieldset>
      ) : null}

      {supportsProxy ? <ConnectionProxySection form={form} setForm={setForm} /> : null}

      {supportsTunnel ? <ConnectionTunnelSection form={form} setForm={setForm} /> : null}
    </div>
  )
}
