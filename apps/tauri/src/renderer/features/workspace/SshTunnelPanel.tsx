import { useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import type { SshForwardRule, SshTunnelSnapshot } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { WorkspaceLoadingState } from '../common/WorkspaceLoadingState'

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
  const [isLoading, setIsLoading] = useState(true)
  const [isCreating, setIsCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const isLoadingRef = useRef(false)
  const isCreatingRef = useRef(false)

  const load = async () => {
    if (isLoadingRef.current) return
    isLoadingRef.current = true
    setIsLoading(true)
    try {
      setTunnels((await window.fileterm?.listSshTunnels(tabId)) ?? [])
      setError(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      isLoadingRef.current = false
      setIsLoading(false)
    }
  }

  useEffect(() => {
    void load()
  }, [tabId])

  const saveDraft = async () => {
    if (isCreatingRef.current) return
    const rule: SshForwardRule = {
      ...draft,
      name: draft.name?.trim() ?? '',
      bindHost: draft.bindHost.trim(),
      bindPort: Number(draft.bindPort),
      ...(draft.kind === 'dynamic'
        ? { targetHost: undefined, targetPort: undefined }
        : { targetHost: draft.targetHost?.trim(), targetPort: Number(draft.targetPort) })
    }
    isCreatingRef.current = true
    setIsCreating(true)
    setError(null)
    try {
      setTunnels(await window.fileterm!.createSshTunnel(tabId, rule))
      setDraft(initialDraft())
      setIsAdding(false)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      isCreatingRef.current = false
      setIsCreating(false)
    }
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
          <button
            aria-label={isLoading ? '正在刷新隧道状态' : '刷新隧道状态'}
            aria-busy={isLoading}
            className="tunnel-icon-button"
            disabled={isLoading}
            type="button"
            onClick={() => void load()}
          >
            <AppIcon name="refresh" size={15} />
          </button>
          <button
            className="primary-button ssh-tunnel-create-button"
            disabled={isCreating}
            type="button"
            onClick={() => {
              setError(null)
              setIsAdding(true)
            }}
          >
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
      <div aria-busy={isLoading} className={`ssh-tunnel-list${isLoading ? ' is-loading' : ''}`}>
        {isLoading ? (
          <WorkspaceLoadingState label="正在加载隧道状态…" />
        ) : tunnels.length ? (
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
              disabled={isCreating}
              type="button"
              onClick={() => {
                setError(null)
                setIsAdding(true)
              }}
            >
              创建第一个隧道
            </button>
          </div>
        )}
      </div>
      {isAdding ? (
        <TunnelEditorDialog
          isSubmitting={isCreating}
          onClose={() => {
            if (!isCreatingRef.current) setIsAdding(false)
          }}
        >
          <form
            aria-busy={isCreating}
            className="ssh-tunnel-form"
            onSubmit={(event) => {
              event.preventDefault()
              void saveDraft()
            }}
          >
            <fieldset className="ssh-tunnel-form-fields" disabled={isCreating}>
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
              {error ? (
                <p className="ssh-tunnel-error ssh-tunnel-dialog-error" role="alert">
                  {error}
                </p>
              ) : null}
              <div className="ssh-tunnel-form-actions">
                <button className="flat-button" type="button" onClick={() => setIsAdding(false)}>
                  取消
                </button>
                <button className="primary-button" type="submit">
                  {isCreating ? <span aria-hidden="true" className="button-spinner" /> : null}
                  {isCreating ? '添加中' : '添加并启动'}
                </button>
              </div>
            </fieldset>
          </form>
        </TunnelEditorDialog>
      ) : null}
    </section>
  )
}

function TunnelEditorDialog({
  children,
  isSubmitting,
  onClose
}: {
  children: ReactNode
  isSubmitting: boolean
  onClose(): void
}) {
  const dialog = (
    <div
      className="modal-backdrop ssh-tunnel-dialog-backdrop"
      onClick={() => {
        if (!isSubmitting) onClose()
      }}
    >
      <div
        aria-busy={isSubmitting}
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
          <CloseButton aria-label="关闭新增隧道窗口" disabled={isSubmitting} onClick={onClose} size="compact" />
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
  const [pendingAction, setPendingAction] = useState<'start' | 'stop' | 'delete' | null>(null)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const pendingActionRef = useRef<typeof pendingAction>(null)
  const update = async (kind: 'start' | 'stop', action: () => Promise<SshTunnelSnapshot[]>) => {
    if (pendingActionRef.current) return
    pendingActionRef.current = kind
    setPendingAction(kind)
    try {
      onChange(await action())
      onError(null)
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      pendingActionRef.current = null
      setPendingAction(null)
    }
  }

  const deleteTunnel = async () => {
    if (pendingActionRef.current) return
    pendingActionRef.current = 'delete'
    setPendingAction('delete')
    setDeleteError(null)
    try {
      onChange(await window.fileterm!.deleteSshTunnel(tabId, tunnel.id))
      onError(null)
      setIsDeleteConfirmOpen(false)
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      setDeleteError(message)
      onError(message)
    } finally {
      pendingActionRef.current = null
      setPendingAction(null)
    }
  }

  return (
    <>
      <article aria-busy={Boolean(pendingAction)} className="ssh-tunnel-row">
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
            disabled={Boolean(pendingAction)}
            type="button"
            onClick={() =>
              void update(running ? 'stop' : 'start', () =>
                running
                  ? window.fileterm!.stopSshTunnel(tabId, tunnel.id)
                  : window.fileterm!.startSshTunnel(tabId, tunnel.id)
              )
            }
          >
            {pendingAction === 'stop' ? '停止中' : pendingAction === 'start' ? '启动中' : running ? '停止' : '启动'}
          </button>
          {tunnel.runtimeOnly ? (
            <button
              type="button"
              className="danger"
              disabled={Boolean(pendingAction)}
              onClick={() => {
                setDeleteError(null)
                setIsDeleteConfirmOpen(true)
              }}
            >
              删除
            </button>
          ) : null}
        </div>
      </article>
      {isDeleteConfirmOpen ? (
        <ConfirmActionDialog
          confirmLabel="删除"
          description={`确定删除隧道“${tunnel.name || `${tunnel.kind.toUpperCase()} ${tunnel.bindPort}`}”吗？删除后 ${tunnel.bindHost}:${tunnel.bindPort} 的监听会立即停止，且无法恢复。`}
          errorMessage={deleteError}
          isSubmitting={pendingAction === 'delete'}
          onClose={() => {
            if (!pendingActionRef.current) {
              setDeleteError(null)
              setIsDeleteConfirmOpen(false)
            }
          }}
          onConfirm={() => void deleteTunnel()}
          title="删除隧道"
        />
      ) : null}
    </>
  )
}
