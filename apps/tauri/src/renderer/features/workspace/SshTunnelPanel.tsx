import { useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import type { SshForwardRule, SshTunnelSnapshot } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { DropdownSelect } from '../common/DropdownSelect'
import { WorkspaceLoadingState } from '../common/WorkspaceLoadingState'
import { t, formatMessage } from '../../i18n'

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
    draft.kind === 'local' ? t.tunnelLocalHint : draft.kind === 'remote' ? t.tunnelRemoteHint : t.tunnelDynamicHint

  return (
    <section className="ssh-tunnel-panel" aria-label="SSH tunnels">
      <header className="ssh-tunnel-panel-header">
        <div>
          <span className="ssh-tunnel-kicker">SSH RUNTIME</span>
          <h2>{t.sshTunnels}</h2>
          <p>{t.sshTunnelsDescription}</p>
        </div>
        <div className="ssh-tunnel-header-actions">
          <button
            aria-label={isLoading ? t.refreshingTunnels : t.refreshTunnels}
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
            <AppIcon name="plus" size={14} /> {t.addTunnel}
          </button>
        </div>
      </header>
      <div className="ssh-tunnel-purpose">
        <AppIcon name="connections" size={18} />
        <p>
          <strong>{t.tunnelPurposeTitle}</strong>
          <span>{t.tunnelPurpose}</span>
        </p>
      </div>
      {error ? <p className="ssh-tunnel-error">{error}</p> : null}
      <div aria-busy={isLoading} className={`ssh-tunnel-list${isLoading ? ' is-loading' : ''}`}>
        {isLoading ? (
          <WorkspaceLoadingState label={t.loadingTunnels} />
        ) : tunnels.length ? (
          tunnels.map((tunnel) => (
            <TunnelRow key={tunnel.id} tabId={tabId} tunnel={tunnel} onChange={setTunnels} onError={setError} />
          ))
        ) : (
          <div className="ssh-tunnel-empty">
            <AppIcon name="connections" size={22} />
            <strong>{t.noRuntimeTunnels}</strong>
            <span>{t.noRuntimeTunnelsDescription}</span>
            <button
              className="flat-button compact ssh-tunnel-secondary-action"
              disabled={isCreating}
              type="button"
              onClick={() => {
                setError(null)
                setIsAdding(true)
              }}
            >
              {t.createFirstTunnel}
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
              <fieldset className="ssh-fieldset ssh-tunnel-fieldset">
                <legend>{t.tunnelGeneral}</legend>
                <div className="ssh-tunnel-field-grid">
                  <label>
                    {t.tunnelType}
                    <DropdownSelect
                      value={draft.kind}
                      options={[
                        { value: 'local', label: t.tunnelLocal },
                        { value: 'remote', label: t.tunnelRemote },
                        { value: 'dynamic', label: t.tunnelDynamic }
                      ]}
                      onChange={(value) =>
                        setDraft((draftValue) => ({ ...draftValue, kind: value as SshForwardRule['kind'] }))
                      }
                    />
                  </label>
                  <p className="ssh-tunnel-kind-hint">{tunnelKindHint}</p>
                  <label className="ssh-tunnel-field-grid__full">
                    {t.tunnelName}
                    <input
                      value={draft.name}
                      placeholder={t.tunnelNamePlaceholder}
                      onChange={(event) => setDraft((value) => ({ ...value, name: event.target.value }))}
                    />
                  </label>
                </div>
              </fieldset>

              <fieldset className="ssh-fieldset ssh-tunnel-fieldset">
                <legend>{draft.kind === 'dynamic' ? t.tunnelListen : t.tunnelForwardRules}</legend>
                <div className="ssh-tunnel-field-grid">
                  <label>
                    {t.tunnelBindHost}
                    <input
                      value={draft.bindHost}
                      required
                      onChange={(event) => setDraft((value) => ({ ...value, bindHost: event.target.value }))}
                    />
                  </label>
                  <label>
                    {t.tunnelBindPort}
                    <input
                      className="ssh-tunnel-port-input"
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
                        {t.tunnelTargetHost}
                        <input
                          value={draft.targetHost}
                          required
                          onChange={(event) => setDraft((value) => ({ ...value, targetHost: event.target.value }))}
                        />
                      </label>
                      <label>
                        {t.tunnelTargetPort}
                        <input
                          className="ssh-tunnel-port-input"
                          min="1"
                          max="65535"
                          required
                          type="number"
                          value={draft.targetPort || ''}
                          onChange={(event) =>
                            setDraft((value) => ({ ...value, targetPort: Number(event.target.value) }))
                          }
                        />
                      </label>
                    </>
                  ) : null}
                </div>
              </fieldset>
            </fieldset>
            {error ? (
              <p className="ssh-tunnel-error ssh-tunnel-dialog-error" role="alert">
                {error}
              </p>
            ) : null}
            <div className="ssh-tunnel-form-actions">
              <button className="flat-button" disabled={isCreating} type="button" onClick={() => setIsAdding(false)}>
                {t.tunnelCancel}
              </button>
              <button className="primary-button" disabled={isCreating} type="submit">
                {isCreating ? <span aria-hidden="true" className="button-spinner" /> : null}
                {isCreating ? t.tunnelAdding : t.tunnelAddAndStart}
              </button>
            </div>
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
            <span id="ssh-tunnel-dialog-title">{t.tunnelDialogTitle}</span>
          </div>
          <CloseButton aria-label={t.closeTunnelDialog} disabled={isSubmitting} onClick={onClose} size="compact" />
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
            {pendingAction === 'stop'
              ? t.tunnelStopping
              : pendingAction === 'start'
                ? t.tunnelStarting
                : running
                  ? t.tunnelStop
                  : t.tunnelStart}
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
              {t.tunnelDelete}
            </button>
          ) : null}
        </div>
      </article>
      {isDeleteConfirmOpen ? (
        <ConfirmActionDialog
          confirmLabel={t.tunnelDelete}
          description={formatMessage(t.tunnelDeleteDescription, {
            name: tunnel.name || `${tunnel.kind.toUpperCase()} ${tunnel.bindPort}`,
            address: `${tunnel.bindHost}:${tunnel.bindPort}`
          })}
          errorMessage={deleteError}
          isSubmitting={pendingAction === 'delete'}
          onClose={() => {
            if (!pendingActionRef.current) {
              setDeleteError(null)
              setIsDeleteConfirmOpen(false)
            }
          }}
          onConfirm={() => void deleteTunnel()}
          title={t.tunnelDeleteTitle}
        />
      ) : null}
    </>
  )
}
