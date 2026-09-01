import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionProxySection({ form, setForm }: { form: CreateProfileInput; setForm: ConnectionFormSetter }) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset">
        <legend>{t.proxyServer}</legend>
        <div className="ssh-grid">
          <label>
            Type:
            <DropdownSelect
              value={form.proxy?.type ?? 'none'}
              options={[
                { value: 'none', label: 'Direct' },
                { value: 'socks5', label: 'SOCKS5' },
                { value: 'http', label: 'HTTP CONNECT' }
              ]}
              onChange={(value) =>
                setForm((prev) => ({
                  ...prev,
                  proxy: {
                    ...(prev.proxy ?? { host: '', port: 1080 }),
                    type: value as 'none' | 'socks5' | 'http'
                  }
                }))
              }
            />
          </label>
          {form.proxy?.type && form.proxy.type !== 'none' ? (
            <>
              <label>
                Host:
                <input
                  value={form.proxy.host}
                  onChange={(event) =>
                    setForm((prev) => ({ ...prev, proxy: { ...prev.proxy!, host: event.target.value } }))
                  }
                />
              </label>
              <label>
                Port:
                <input
                  inputMode="numeric"
                  value={form.proxy.port}
                  onChange={(event) =>
                    setForm((prev) => ({
                      ...prev,
                      proxy: { ...prev.proxy!, port: Number(event.target.value) }
                    }))
                  }
                />
              </label>
              <label>
                Username:
                <input
                  value={form.proxy.username ?? ''}
                  onChange={(event) =>
                    setForm((prev) => ({ ...prev, proxy: { ...prev.proxy!, username: event.target.value } }))
                  }
                />
              </label>
              <label>
                Password:
                <input
                  type="password"
                  value={form.proxyPassword ?? ''}
                  onChange={(event) => setForm((prev) => ({ ...prev, proxyPassword: event.target.value }))}
                />
              </label>
            </>
          ) : null}
        </div>
      </fieldset>
    </div>
  )
}
