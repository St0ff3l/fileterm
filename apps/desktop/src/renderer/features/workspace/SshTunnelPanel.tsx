import { useEffect, useState } from 'react'
import type { SshForwardRule, SshTunnelSnapshot } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'

const initialDraft = (): SshForwardRule => ({
  id: globalThis.crypto.randomUUID(),
  name: '',
  kind: 'local',
  bindHost: '127.0.0.1',
  bindPort: 0,
  targetHost: '127.0.0.1',
  targetPort: 0,
  autoStart: false
})

export function SshTunnelPanel({ tabId }: { tabId: string }) {
  const [tunnels, setTunnels] = useState<SshTunnelSnapshot[]>([])
  const [draft, setDraft] = useState(initialDraft)
  const [isAdding, setIsAdding] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    try {
      setTunnels((await window.fileterm?.listSshTunnels(tabId)) ?? [])
      setError(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }

  useEffect(() => {
    void load()
  }, [tabId])

  const run = async (action: () => Promise<SshTunnelSnapshot[]>) => {
    try {
      setTunnels(await action())
      setError(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }

  const saveDraft = () => {
    const rule: SshForwardRule = {
      ...draft,
      name: draft.name?.trim() ?? '',
      bindHost: draft.bindHost.trim(),
      bindPort: Number(draft.bindPort),
      ...(draft.kind === 'dynamic'
        ? { targetHost: undefined, targetPort: undefined }
        : { targetHost: draft.targetHost?.trim(), targetPort: Number(draft.targetPort) })
    }
    void run(async () => {
      const created = await window.fileterm!.createSshTunnel(tabId, rule)
      setDraft(initialDraft())
      setIsAdding(false)
      return created
    })
  }

  return (
    <section className="ssh-tunnel-panel" aria-label="SSH tunnels">
      <header className="ssh-tunnel-panel-header">
        <div>
          <span className="ssh-tunnel-kicker">SSH RUNTIME</span>
          <h2>隧道</h2>
          <p>只影响当前工作区标签；连接关闭后自动回收。</p>
        </div>
        <div className="ssh-tunnel-header-actions">
          <button aria-label="刷新隧道状态" className="tunnel-icon-button" type="button" onClick={() => void load()}>
            <AppIcon name="refresh" size={15} />
          </button>
          <button className="tunnel-add-button" type="button" onClick={() => setIsAdding((value) => !value)}>
            <AppIcon name="plus" size={14} /> 新增隧道
          </button>
        </div>
      </header>
      {isAdding ? (
        <form
          className="ssh-tunnel-form"
          onSubmit={(event) => {
            event.preventDefault()
            saveDraft()
          }}
        >
          <label>
            类型
            <select
              value={draft.kind}
              onChange={(event) =>
                setDraft((value) => ({ ...value, kind: event.target.value as SshForwardRule['kind'] }))
              }
            >
              <option value="local">本地 -L</option>
              <option value="remote">远程 -R</option>
              <option value="dynamic">动态 -D (SOCKS5)</option>
            </select>
          </label>
          <label>
            名称
            <input
              value={draft.name}
              placeholder="例如：数据库"
              onChange={(event) => setDraft((value) => ({ ...value, name: event.target.value }))}
            />
          </label>
          <label>
            监听地址
            <input
              value={draft.bindHost}
              required
              onChange={(event) => setDraft((value) => ({ ...value, bindHost: event.target.value }))}
            />
          </label>
          <label>
            监听端口
            <input
              min="1"
              max="65535"
              required
              type="number"
              value={draft.bindPort || ''}
              onChange={(event) => setDraft((value) => ({ ...value, bindPort: Number(event.target.value) }))}
            />
          </label>
          {draft.kind !== 'dynamic' ? (
            <>
              <label>
                目标主机
                <input
                  value={draft.targetHost}
                  required
                  onChange={(event) => setDraft((value) => ({ ...value, targetHost: event.target.value }))}
                />
              </label>
              <label>
                目标端口
                <input
                  min="1"
                  max="65535"
                  required
                  type="number"
                  value={draft.targetPort || ''}
                  onChange={(event) => setDraft((value) => ({ ...value, targetPort: Number(event.target.value) }))}
                />
              </label>
            </>
          ) : null}
          <div className="ssh-tunnel-form-actions">
            <button type="button" onClick={() => setIsAdding(false)}>
              取消
            </button>
            <button className="tunnel-add-button" type="submit">
              添加
            </button>
          </div>
        </form>
      ) : null}
      {error ? <p className="ssh-tunnel-error">{error}</p> : null}
      <div className="ssh-tunnel-list">
        {tunnels.length ? (
          tunnels.map((tunnel) => (
            <TunnelRow key={tunnel.id} tabId={tabId} tunnel={tunnel} onChange={setTunnels} onError={setError} />
          ))
        ) : (
          <div className="ssh-tunnel-empty">暂无运行时隧道。已保存的自动启动规则会在 SSH 连接建立后显示在这里。</div>
        )}
      </div>
    </section>
  )
}

function TunnelRow({
  tabId,
  tunnel,
  onChange,
  onError
}: {
  tabId: string
  tunnel: SshTunnelSnapshot
  onChange(value: SshTunnelSnapshot[]): void
  onError(value: string | null): void
}) {
  const running = tunnel.status === 'running' || tunnel.status === 'starting'
  const target = tunnel.kind === 'dynamic' ? 'SOCKS5' : `${tunnel.targetHost}:${tunnel.targetPort}`
  const update = async (action: () => Promise<SshTunnelSnapshot[]>) => {
    try {
      onChange(await action())
      onError(null)
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause))
    }
  }
  return (
    <article className="ssh-tunnel-row">
      <span className={`ssh-tunnel-status is-${tunnel.status}`} aria-label={tunnel.status} />
      <div className="ssh-tunnel-description">
        <strong>{tunnel.name || `${tunnel.kind.toUpperCase()} ${tunnel.bindPort}`}</strong>
        <span>
          {tunnel.kind.toUpperCase()} · {tunnel.bindHost}:{tunnel.bindPort} → {target}
        </span>
        {tunnel.error ? <em>{tunnel.error}</em> : null}
      </div>
      <span className="ssh-tunnel-state">{tunnel.status}</span>
      <div className="ssh-tunnel-actions">
        <button
          type="button"
          onClick={() =>
            void update(() =>
              running
                ? window.fileterm!.stopSshTunnel(tabId, tunnel.id)
                : window.fileterm!.startSshTunnel(tabId, tunnel.id)
            )
          }
        >
          {running ? '停止' : '启动'}
        </button>
        {tunnel.runtimeOnly ? (
          <button
            type="button"
            className="danger"
            onClick={() => void update(() => window.fileterm!.deleteSshTunnel(tabId, tunnel.id))}
          >
            删除
          </button>
        ) : null}
      </div>
    </article>
  )
}
