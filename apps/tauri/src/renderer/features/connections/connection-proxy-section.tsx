import { useMemo, useState } from 'react'
import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { AppIcon } from '../common/app-icon'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { ProxyEditDialog } from '../proxies/proxy-edit-dialog'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionProxySection({ form, setForm }: { form: CreateProfileInput; setForm: ConnectionFormSetter }) {
  const { proxies, saveProxy, testProxy } = useProxyLibrary()

  const selectedProxy = useMemo(() => {
    if (!form.proxyProfileId) return null
    return proxies.find((p) => p.id === form.proxyProfileId) ?? null
  }, [form.proxyProfileId, proxies])

  const selectValue = useMemo(() => {
    if (form.proxyProfileId) return `saved:${form.proxyProfileId}`
    if (form.proxy?.type && form.proxy.type !== 'none') return 'custom'
    return 'none'
  }, [form.proxyProfileId, form.proxy?.type])

  const options = useMemo(() => {
    const list: Array<{ value: string; label: string }> = [{ value: 'none', label: t.proxySourceDirect }]

    if (form.proxyProfileId && !proxies.some((proxy) => proxy.id === form.proxyProfileId)) {
      list.push({ value: `saved:${form.proxyProfileId}`, label: t.proxySourceSaved })
    }

    for (const p of proxies) {
      list.push({
        value: `saved:${p.id}`,
        label: `${p.name} (${p.type.toUpperCase()} · ${p.host}:${p.port})`
      })
    }

    list.push({ value: 'custom', label: t.proxySourceCustom })
    return list
  }, [form.proxyProfileId, proxies])

  const handleSelectChange = (value: string) => {
    if (value === 'none') {
      setForm((prev) => ({
        ...prev,
        proxyProfileId: undefined,
        proxy: undefined,
        proxyPassword: ''
      }))
    } else if (value === 'custom') {
      setForm((prev) => ({
        ...prev,
        proxyProfileId: undefined,
        proxy: prev.proxy?.type && prev.proxy.type !== 'none' ? prev.proxy : { type: 'socks5', host: '', port: 1080 }
      }))
    } else if (value.startsWith('saved:')) {
      const proxyId = value.slice(6)
      const target = proxies.find((p) => p.id === proxyId)
      if (target) {
        setForm((prev) => ({
          ...prev,
          proxyProfileId: target.id,
          proxy: {
            type: target.type,
            host: target.host,
            port: target.port,
            username: target.username
          },
          proxyPassword: ''
        }))
      }
    }
  }

  const [isEditingProxy, setIsEditingProxy] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  return (
    <fieldset className="ssh-fieldset">
      <legend className="proxy-section-legend">
        <span>{t.proxyServer}</span>
      </legend>
      <div className="tunnel-intro proxy-library-hint">
        <AppIcon name="info" size={15} className="tunnel-intro__icon" />
        <p>{t.networkProfilesLocationHint}</p>
      </div>
      <div className="ssh-grid">
        <label className="span-2">
          {t.proxySource}:
          <DropdownSelect value={selectValue} options={options} onChange={handleSelectChange} />
        </label>

        {selectValue.startsWith('saved:') ? (
          <div className="span-2 proxy-saved-selected-card">
            <div className="proxy-saved-selected-header">
              <div className="proxy-saved-selected-info">
                <span
                  className={`proxy-type-badge proxy-type-badge--${selectedProxy?.type ?? form.proxy?.type ?? 'socks5'}`}
                >
                  {(selectedProxy?.type ?? form.proxy?.type ?? 'SOCKS5').toUpperCase()}
                </span>
                <strong className="proxy-saved-selected-name">{selectedProxy?.name ?? '已选全局代理'}</strong>
                <span className="proxy-saved-selected-endpoint">
                  {selectedProxy
                    ? `${selectedProxy.host}:${selectedProxy.port}`
                    : form.proxy
                      ? `${form.proxy.host}:${form.proxy.port}`
                      : ''}
                </span>
              </div>
              {selectedProxy ? (
                <button
                  type="button"
                  className="flat-button proxy-card-edit-btn"
                  onClick={() => setIsEditingProxy(true)}
                  title={t.edit}
                >
                  <AppIcon name="edit" size={13} />
                  <span>{t.edit}</span>
                </button>
              ) : null}
            </div>
            <div className="proxy-saved-selected-note">
              <AppIcon name="info" size={14} />
              <span>已绑定全局代理配置，该连接将自动通过此代理中继网络请求。</span>
            </div>
          </div>
        ) : null}

        {selectValue === 'custom' ? (
          <>
            <label>
              {t.proxyProtocol}:
              <DropdownSelect
                value={form.proxy?.type ?? 'socks5'}
                options={[
                  { value: 'socks5', label: t.proxyProtocolSocks5 },
                  { value: 'http', label: t.proxyProtocolHttpConnect }
                ]}
                onChange={(value) =>
                  setForm((prev) => ({
                    ...prev,
                    proxy: {
                      ...(prev.proxy ?? { host: '', port: 1080 }),
                      type: value as 'socks5' | 'http'
                    }
                  }))
                }
              />
            </label>
            <label>
              {t.host}:
              <input
                value={form.proxy?.host ?? ''}
                onChange={(event) =>
                  setForm((prev) => ({
                    ...prev,
                    proxy: { ...(prev.proxy ?? { type: 'socks5', port: 1080 }), host: event.target.value }
                  }))
                }
              />
            </label>
            <label>
              {t.port}:
              <input
                inputMode="numeric"
                value={form.proxy?.port ?? 1080}
                onChange={(event) =>
                  setForm((prev) => ({
                    ...prev,
                    proxy: { ...(prev.proxy ?? { type: 'socks5', host: '' }), port: Number(event.target.value) }
                  }))
                }
              />
            </label>
            <label>
              {t.username}:
              <input
                value={form.proxy?.username ?? ''}
                onChange={(event) =>
                  setForm((prev) => ({
                    ...prev,
                    proxy: { ...(prev.proxy ?? { type: 'socks5', host: '', port: 1080 }), username: event.target.value }
                  }))
                }
              />
            </label>
            <label>
              {t.password}:
              <input
                type="password"
                value={form.proxyPassword ?? ''}
                onChange={(event) => setForm((prev) => ({ ...prev, proxyPassword: event.target.value }))}
              />
            </label>
          </>
        ) : null}
      </div>

      {selectedProxy ? (
        <ProxyEditDialog
          isOpen={isEditingProxy}
          initialProxy={selectedProxy}
          isSubmitting={isSubmitting}
          onClose={() => setIsEditingProxy(false)}
          onSave={async (input) => {
            setIsSubmitting(true)
            try {
              const saved = await saveProxy(input)
              setIsEditingProxy(false)
              setForm((prev) => ({
                ...prev,
                proxyProfileId: saved.id,
                proxy: {
                  type: saved.type,
                  host: saved.host,
                  port: saved.port,
                  username: saved.username
                }
              }))
            } finally {
              setIsSubmitting(false)
            }
          }}
          onTest={testProxy}
        />
      ) : null}
    </fieldset>
  )
}
