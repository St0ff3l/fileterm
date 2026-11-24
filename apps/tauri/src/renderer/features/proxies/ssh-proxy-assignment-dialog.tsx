import { useEffect, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { ConnectionProfile, FileTermDesktopApi, ProxyProfile, SshProxySource } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { Dialog } from '../../components/common/dialog/dialog'
import { Input } from '../../components/common/input/input'
import { formatMessage, t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { SelectionControl } from '../common/selection-control'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { proxyAssignmentChanges, proxyAssignmentRows } from './ssh-proxy-assignments'
import './ssh-proxy-assignment-dialog.css'

export function SshProxyAssignmentDialog({
  proxy,
  proxies,
  profiles,
  onClose,
  onSaved
}: {
  proxy: ProxyProfile
  proxies: ProxyProfile[]
  profiles: ConnectionProfile[]
  onClose(): void
  onSaved(snapshotRefreshed: boolean): void
}) {
  const [baseline, setBaseline] = useState(profiles)
  const [edits, setEdits] = useState(() => new Map<string, boolean>())
  const [query, setQuery] = useState('')
  const [review, setReview] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const inFlight = useRef(false)
  const scrollRef = useRef<HTMLDivElement>(null)
  const headingRef = useRef<HTMLHeadingElement>(null)
  const titleId = useId()
  const rows = proxyAssignmentRows(baseline, proxy.id)
  const changes = proxyAssignmentChanges(rows, edits)
  const changedIds = new Set(changes.map(({ profileId }) => profileId))
  const changedRows = rows.filter(({ profile }) => changedIds.has(profile.id))
  const replacesCustomProxy = changes.some(({ expected }) => expected.kind === 'custom')
  const proxyName = (id: string) => proxies.find((item) => item.id === id)?.name ?? id
  const sourceLabel = (source: SshProxySource) => {
    if (source.kind === 'saved') return proxyName(source.proxyId)
    if (source.kind === 'custom') return `${t.proxySourceCustom} · ${source.host}:${source.port}`
    return t.proxyAssignmentDirect
  }
  const search = query.trim().toLowerCase()
  const visibleRows = rows.filter(({ profile, source }) =>
    [profile.name, profile.group, profile.host, sourceLabel(source)].some((value) =>
      value?.toLowerCase().includes(search)
    )
  )

  useEffect(() => {
    if (!review) headingRef.current?.focus()
  }, [review])

  const run = async (action: (api: FileTermDesktopApi) => Promise<void>) => {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    setError('')
    try {
      const api = window.fileterm
      if (!api) throw new Error(t.proxyAssignmentApiUnavailable)
      await action(api)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
      setReview(false)
    } finally {
      inFlight.current = false
      setBusy(false)
    }
  }

  const refresh = () =>
    void run(async (api) => {
      const latest = await api.getConnectionLibrary()
      setBaseline(latest.profiles)
    })

  if (review) {
    return (
      <ConfirmActionDialog
        cancelLabel={t.proxyAssignmentBack}
        className="proxy-assignment-review"
        confirmLabel={t.save}
        confirmVariant={replacesCustomProxy ? 'danger' : 'primary'}
        description={
          <>
            <div className="proxy-assignment-scroll">
              <div className="proxy-assignment-list" ref={scrollRef}>
                {changedRows.map(({ profile, source, assigned }) => (
                  <div className="proxy-assignment-review-row" key={profile.id}>
                    <span className="proxy-assignment-summary">
                      <strong title={profile.name}>{profile.name}</strong>
                      <small title={profile.host}>{profile.host}</small>
                    </span>
                    <span className="proxy-assignment-route-change">
                      <span>{sourceLabel(source)}</span>
                      <span aria-hidden="true">→</span>
                      <strong>{assigned ? t.proxyAssignmentDirect : proxyName(proxy.id)}</strong>
                    </span>
                  </div>
                ))}
              </div>
              <VerticalScrollbar scrollRef={scrollRef} />
            </div>
            {replacesCustomProxy ? <p>{t.proxyAssignmentCustomWarning}</p> : null}
            <p>{t.proxyAssignmentAppliesOnConnect}</p>
          </>
        }
        isSubmitting={busy}
        onClose={() => setReview(false)}
        onConfirm={() =>
          void run(async (api) => {
            onSaved(await api.assignSshProxy({ proxyId: proxy.id, changes }))
          })
        }
        title={t.proxyAssignmentReview}
      />
    )
  }

  return createPortal(
    <Dialog
      aria-busy={busy}
      aria-labelledby={titleId}
      className="proxy-assignment-dialog"
      closeOnBackdropClick={!busy}
      closeOnEscape={!busy}
      isOpen
      onClose={onClose}
      showCloseButton={false}
      title={
        <div className="proxy-assignment-heading">
          <AppIcon name="shield" size={16} />
          <h2
            className="ft-dialog__title"
            id={titleId}
            ref={headingRef}
            tabIndex={-1}
            title={`${proxyName(proxy.id)} · ${proxy.host}:${proxy.port}`}
          >
            {t.proxyManageAssignments} <span>· {proxyName(proxy.id)}</span>
          </h2>
        </div>
      }
      footer={
        <>
          <span className="proxy-assignment-count" role="status">
            {formatMessage(t.proxyAssignmentSelectedCount, {
              count: rows.filter(({ profile, assigned, blocked }) =>
                blocked ? assigned : (edits.get(profile.id) ?? assigned)
              ).length
            })}
          </span>
          <Button disabled={busy} onClick={onClose}>
            {t.cancel}
          </Button>
          <Button disabled={busy || !changes.length} onClick={() => setReview(true)} type="button" variant="primary">
            {t.proxyAssignmentReview}
          </Button>
        </>
      }
    >
      <div className="proxy-assignment-toolbar">
        <Input
          aria-label={t.proxyAssignmentSearch}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t.proxyAssignmentSearch}
          prefix={<AppIcon name="search" size={14} />}
          sizeVariant="sm"
          type="search"
          value={query}
        />
        <Button
          aria-label={t.refresh}
          className="proxy-assignment-refresh"
          disabled={busy}
          icon={<AppIcon name="refresh" size={15} />}
          onClick={refresh}
          size="sm"
          title={t.refresh}
        />
      </div>
      {error ? (
        <div className="modal-error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="proxy-assignment-table">
        <div aria-hidden="true" className="proxy-assignment-columns">
          <span>{t.connection}</span>
          <span>{t.proxyAssignmentCurrentRoute}</span>
        </div>
        <div className="proxy-assignment-scroll">
          <div className="proxy-assignment-list" ref={scrollRef}>
            {visibleRows.map(({ profile, source, assigned, blocked }) => (
              <label className="proxy-assignment-row" key={profile.id}>
                <SelectionControl
                  checked={blocked ? assigned : (edits.get(profile.id) ?? assigned)}
                  disabled={busy || blocked}
                  onChange={(event) => {
                    const checked = event.target.checked
                    setEdits((current) => {
                      const next = new Map(current)
                      if (checked === assigned) next.delete(profile.id)
                      else next.set(profile.id, checked)
                      return next
                    })
                  }}
                  type="checkbox"
                />
                <span className="proxy-assignment-summary">
                  <strong title={profile.name}>{profile.name}</strong>
                  <small title={[profile.group, profile.host].filter(Boolean).join(' · ')}>
                    {[profile.group, profile.host].filter(Boolean).join(' · ')}
                  </small>
                </span>
                <span className="proxy-assignment-route">
                  <span title={sourceLabel(source)}>{sourceLabel(source)}</span>
                  {blocked ? (
                    <small title={t.proxyAssignmentJumpConflict}>{t.proxyAssignmentJumpConflict}</small>
                  ) : null}
                </span>
              </label>
            ))}
            {!visibleRows.length ? <p className="proxy-assignment-empty">{t.noMatchingConnections}</p> : null}
          </div>
          <VerticalScrollbar scrollRef={scrollRef} />
        </div>
      </div>
    </Dialog>,
    document.body
  )
}
