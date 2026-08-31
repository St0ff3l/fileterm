import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import type { ConnectionFormSetter } from './connection-modal-utils'
import { TunnelRuleEditor } from './tunnel-rule-editor'

export function ConnectionTunnelSection({
  form,
  setForm
}: {
  form: CreateProfileInput
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset tunnel-fieldset">
        <legend>{t.tunnel}</legend>
        <div className="tunnel-intro">
          <span className="material-symbols-outlined">lan</span>
          <p>{t.tunnelAutoStartHint}</p>
        </div>
        <div className="tunnel-rule-list">
          {(form.forwards ?? []).map((rule, index) => (
            <TunnelRuleEditor
              key={rule.id}
              index={index}
              rule={rule}
              onChange={(patch) =>
                setForm((prev) => ({
                  ...prev,
                  forwards: prev.forwards?.map((item) => (item.id === rule.id ? { ...item, ...patch } : item))
                }))
              }
              onRemove={() =>
                setForm((prev) => ({
                  ...prev,
                  forwards: prev.forwards?.filter((item) => item.id !== rule.id)
                }))
              }
            />
          ))}
        </div>
        <button
          type="button"
          className="tunnel-add-button"
          onClick={() =>
            setForm((prev) => ({
              ...prev,
              forwards: [
                ...(prev.forwards ?? []),
                {
                  id: crypto.randomUUID(),
                  kind: 'local',
                  bindHost: '127.0.0.1',
                  bindPort: 0,
                  targetHost: '127.0.0.1',
                  targetPort: 0,
                  autoStart: true
                }
              ]
            }))
          }
        >
          <span className="material-symbols-outlined">add</span>
          {t.addConnectionTunnel}
        </button>
      </fieldset>
    </div>
  )
}
