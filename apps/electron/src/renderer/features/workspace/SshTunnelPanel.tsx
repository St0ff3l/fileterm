import { useEffect, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import type { SshForwardRule, SshTunnelSnapshot } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'

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

  const tunnelKindHint =
    draft.kind === 'local'
      ? '本机应用访问 127.0.0.1:监听端口时，流量会通过 SSH 转到目标主机。适合访问服务器内网的数据库或管理后台。'
      : draft.kind === 'remote'
        ? '远端主机访问监听地址和端口时，流量会通过 SSH 回连到目标主机。适合把本机开发服务临时暴露给远端。'
        : '在本机创建 SOCKS5 代理。将浏览器或开发工具指向监听地址和端口后，流量会通过当前 SSH 连接转发。'

  return (
    <section className="ssh-tunnel-panel" aria-label="SSH tunnels">
      <header className="ssh-tunnel-panel-header">
        <div>
          <span className="ssh-tunnel-kicker">SSH RUNTIME</span>
          <h2>SSH 隧道</h2>
          <p>把端口接入当前 SSH 的加密通道，关闭标签后自动回收。</p>
        </div>
        <div className="ssh-tunnel-header-actions">
          <button aria-label="刷新隧道状态" className="tunnel-icon-button" type="button" onClick={() => void load()}>
            <AppIcon name="refresh" size={15} />
          </button>
          <button className="primary-button ssh-tunnel-create-button" type="button" onClick={() => setIsAdding(true)}>
            <AppIcon name="plus" size={14} /> 新增隧道
          </button>
        </div>
      </header>
      <div className="ssh-tunnel-purpose">
        <AppIcon name="connections" size={18} />
        <p>
          <strong>把一个端口安全地穿过 SSH 连接。</strong>
          <span>
            本地 <code>-L</code>：本机访问服务器内网服务；远程 <code>-R</code>：远端回连本机；动态 <code>-D</code>：本机
            SOCKS5 代理。
          </span>
        </p>
      </div>
      {error ? <p className="ssh-tunnel-error">{error}</p> : null}
      <div className="ssh-tunnel-list">
        {tunnels.length ? (
          tunnels.map((tunnel) => (
            <TunnelRow key={tunnel.id} tabId={tabId} tunnel={tunnel} onChange={setTunnels} onError={setError} />
          ))
        ) : (
          <div className="ssh-tunnel-empty">
            <AppIcon name="connections" size={22} />
            <strong>还没有运行时隧道</strong>
            <span>创建后会立即绑定端口，并在当前 SSH 会话断开时自动停止。</span>
            <button
              className="flat-button compact ssh-tunnel-secondary-action"
              type="button"
              onClick={() => setIsAdding(true)}
            >
              创建第一个隧道
            </button>
          </div>
        )}
      </div>
      {isAdding ? (
        <TunnelEditorDialog onClose={() => setIsAdding(false)}>
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
            <p className="ssh-tunnel-kind-hint">{tunnelKindHint}</p>
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
              <button className="flat-button" type="button" onClick={() => setIsAdding(false)}>
                取消
              </button>
              <button className="primary-button" type="submit">
                添加并启动
              </button>
            </div>
          </form>
        </TunnelEditorDialog>
      ) : null}
    </section>
  )
}

function TunnelEditorDialog({ children, onClose }: { children: ReactNode; onClose(): void }) {
  const dialog = (
    <div className="modal-backdrop ssh-tunnel-dialog-backdrop" onClick={onClose}>
      <div
        aria-labelledby="ssh-tunnel-dialog-title"
        aria-modal="true"
        className="modal-card ssh-tunnel-dialog"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header className="ssh-tunnel-dialog-header">
          <div className="ssh-tunnel-dialog-title">
            <AppIcon name="connections" size={16} />
            <span id="ssh-tunnel-dialog-title">新增运行时隧道</span>
          </div>
          <CloseButton aria-label="关闭新增隧道窗口" onClick={onClose} size="compact" />
        </header>
        {children}
      </div>
    </div>
  )

  return typeof document === 'undefined' ? dialog : createPortal(dialog, document.body)
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
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false)
  const [isDeleting, setIsDeleting] = useState(false)
  const update = async (action: () => Promise<SshTunnelSnapshot[]>) => {
    try {
      onChange(await action())
      onError(null)
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause))
    }
  }

  const deleteTunnel = async () => {
    setIsDeleting(true)
    try {
      onChange(await window.fileterm!.deleteSshTunnel(tabId, tunnel.id))
      onError(null)
      setIsDeleteConfirmOpen(false)
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setIsDeleting(false)
    }
  }

  return (
    <>
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
            <button type="button" className="danger" onClick={() => setIsDeleteConfirmOpen(true)}>
              删除
            </button>
          ) : null}
        </div>
      </article>
      {isDeleteConfirmOpen ? (
        <ConfirmActionDialog
          confirmLabel="删除"
          description={`确定删除隧道“${tunnel.name || `${tunnel.kind.toUpperCase()} ${tunnel.bindPort}`}”吗？删除后 ${tunnel.bindHost}:${tunnel.bindPort} 的监听会立即停止，且无法恢复。`}
          isSubmitting={isDeleting}
          onClose={() => {
            if (!isDeleting) setIsDeleteConfirmOpen(false)
          }}
          onConfirm={() => void deleteTunnel()}
          title="删除隧道"
        />
      ) : null}
    </>
  )
}
