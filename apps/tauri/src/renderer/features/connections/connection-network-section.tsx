import type { ConnectionProfile, CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { ConnectionProxySection } from './connection-proxy-section'
import { ConnectionTunnelSection } from './connection-tunnel-section'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionNetworkSection({
  form,
  jumpHosts,
  routingMode,
  setForm,
  setRoutingMode
}: {
  form: CreateProfileInput
  jumpHosts: ConnectionProfile[]
  routingMode: 'direct' | 'jump'
  setForm: ConnectionFormSetter
  setRoutingMode(value: 'direct' | 'jump'): void
}) {
  const supportsRouting = form.type === 'ssh'
  const supportsProxy = form.type === 'ssh' || form.type === 'telnet' || form.type === 'ftp'
  const supportsTunnel = form.type === 'ssh'

  return (
    <div className="ssh-form-page">
      {supportsRouting ? (
        <fieldset className="ssh-fieldset">
          <legend>{t.networkRouting}</legend>
          <div className="advanced-toggle-list network-routing-list">
            <div className="advanced-toggle-row network-routing-row">
              <span className="network-routing-row__name">{t.route}</span>
              <div className="network-routing-modes" role="radiogroup" aria-label={t.route}>
                <button
                  aria-checked={routingMode === 'direct'}
                  className={routingMode === 'direct' ? 'is-active' : undefined}
                  onClick={() => {
                    setRoutingMode('direct')
                    setForm((prev) => ({ ...prev, jumpProfileId: undefined }))
                  }}
                  role="radio"
                  type="button"
                >
                  {t.direct}
                </button>
                <button
                  aria-checked={routingMode === 'jump'}
                  className={routingMode === 'jump' ? 'is-active' : undefined}
                  onClick={() => setRoutingMode('jump')}
                  role="radio"
                  type="button"
                >
                  {t.viaJumpHost}
                </button>
              </div>
            </div>
            {routingMode === 'jump' && jumpHosts.length ? (
              <>
                <label className="advanced-toggle-row network-routing-row">
                  <span className="network-routing-row__name">{t.jumpHost}</span>
                  <DropdownSelect
                    className="network-routing-select"
                    placeholder={t.selectJumpHost}
                    value={form.jumpProfileId ?? ''}
                    options={[
                      { value: '', label: t.selectJumpHost, disabled: true },
                      ...jumpHosts.map((profile) => ({
                        value: profile.id,
                        label: `${profile.name} (${profile.host})`
                      }))
                    ]}
                    onChange={(value) => setForm((prev) => ({ ...prev, jumpProfileId: value || undefined }))}
                  />
                </label>
                <p className="network-routing-hint">{t.jumpHostHint}</p>
              </>
            ) : null}
            {routingMode === 'jump' && !jumpHosts.length ? (
              <p className="network-routing-empty">{t.noAvailableJumpHost}</p>
            ) : null}
          </div>
        </fieldset>
      ) : null}

      {supportsProxy ? <ConnectionProxySection form={form} setForm={setForm} /> : null}

      {supportsTunnel ? <ConnectionTunnelSection form={form} setForm={setForm} /> : null}
    </div>
  )
}
