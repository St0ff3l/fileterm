import { useCallback, useEffect, useRef, useState } from 'react'
import type { McpCallLogEntry, McpCallLogPage } from '@fileterm/core'
import { writeClipboardText } from '../../app/app-utils'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../../components/common/confirm-action-dialog/confirm-action-dialog'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import './mcp-call-log.css'

const PAGE_SIZE = 60

function approvalLabel(approval: string): string {
  switch (approval) {
    case 'user':
      return t.mcpCallApprovalUser
    case 'blocked-by-policy':
      return t.mcpCallApprovalBlocked
    case 'not-required':
      return t.mcpCallApprovalPassive
    case 'pending':
      return t.mcpCallApprovalPending
    case 'rejected':
      return t.mcpCallApprovalRejected
    case 'dismissed':
      return t.mcpCallApprovalDismissed
    case 'timed-out':
      return t.mcpCallApprovalTimedOut
    case 'delegated-to-terminal':
      return t.mcpCallApprovalDelegated
    case 'approval-error':
      return t.mcpCallApprovalError
    default:
      return t.mcpCallLegacyUnknown
  }
}

function resultLabel(entry: McpCallLogEntry): string {
  switch (entry.status) {
    case 'started':
      return t.mcpCallResultStarted
    case 'accepted':
      return t.mcpCallResultAccepted
    case 'outcome-uncertain':
      return t.mcpCallResultUncertain
    case 'command-succeeded':
      return t.mcpCallResultCommandSucceeded
    case 'command-failed':
      return t.mcpCallResultCommandFailed
    case 'call-succeeded':
      return t.mcpCallResultSucceeded
    case 'call-failed':
      return t.mcpCallResultFailed
    default:
      return t.mcpCallLegacyUnknown
  }
}

function resultClass(entry: McpCallLogEntry): string {
  switch (entry.status) {
    case 'command-failed':
    case 'call-failed':
      return 'is-failed'
    case 'outcome-uncertain':
      return 'is-uncertain'
    default:
      return ''
  }
}

function summarizeParams(entry: McpCallLogEntry): string {
  const target = entry.target
  const params = entry.params
  const host = target?.host ? `${target.host}${target.port ? `:${target.port}` : ''}` : ''
  const path = ['path', 'target_path', 'remote_path', 'local_path', 'cwd']
    .map((key) => params[key])
    .find((value) => typeof value === 'string' && value.trim())
  return [host || target?.profileId, path || params.profile_id || params.tab_id].filter(Boolean).join(' · ')
}

function formatClock(ts: string): string {
  const date = new Date(ts)
  if (Number.isNaN(date.getTime())) return ts
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

export function McpCallLogPage({ onTotalChange }: { onTotalChange(total: number): void }) {
  const [page, setPage] = useState<McpCallLogPage | null>(null)
  const [loading, setLoading] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [selected, setSelected] = useState<McpCallLogEntry | null>(null)
  const [copying, setCopying] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const pendingLoad = useRef(false)
  const bodyRef = useRef<HTMLDivElement>(null)
  const detailRef = useRef<HTMLDivElement>(null)

  const load = useCallback(async (before?: string) => {
    if (pendingLoad.current) return
    pendingLoad.current = true
    setLoading(true)
    setLoadError(null)
    try {
      const next = await window.fileterm?.readMcpCallLog(PAGE_SIZE, 0, before)
      if (!next) throw new Error('FileTerm desktop API is unavailable')
      setPage((previous) => (before && previous ? { ...next, items: [...previous.items, ...next.items] } : next))
      if (!before && bodyRef.current) bodyRef.current.scrollTop = 0
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error))
    } finally {
      pendingLoad.current = false
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    onTotalChange(page?.total ?? 0)
  }, [onTotalChange, page?.total])

  const entries = page?.items ?? []
  const initialLoading = page === null
  const select = (entry: McpCallLogEntry) => {
    setCopyError(null)
    setSelected(entry)
  }

  return (
    <section className="background-sessions-page manager-inline connection-manager-modal mcp-call-log-page">
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <AppIcon name="call-log" size={20} />
          <span>{t.mcpCallRecordsTitle}</span>
        </span>
        <button
          aria-label={t.refresh}
          className="manager-icon-action"
          disabled={loading}
          title={t.refresh}
          type="button"
          onClick={() => void load()}
        >
          <AppIcon name="refresh" size={16} />
        </button>
      </div>

      {loadError ? (
        <p className="mcp-call-log-error" role="alert">
          {loadError}
        </p>
      ) : null}

      <div className="connection-manager-layout background-sessions-layout">
        <section className="connection-manager-main">
          <div
            className="manager-table connection-manager-table mcp-call-log-table"
            role="table"
            aria-label={t.mcpCallRecordsTitle}
          >
            <div className="manager-head" role="row">
              <span role="columnheader">{t.mcpCallColumnTime}</span>
              <span role="columnheader">{t.mcpCallColumnAction}</span>
              <span role="columnheader">{t.mcpCallColumnApproval}</span>
              <span role="columnheader">{t.mcpCallColumnDetail}</span>
              <span role="columnheader">{t.mcpCallColumnResult}</span>
            </div>
            <div className="mcp-call-log-scroll">
              <div
                className="manager-body connection-manager-body mcp-call-log-body"
                ref={bodyRef}
                role="rowgroup"
                aria-busy={loading}
              >
                {entries.map((entry) => (
                  <div
                    className={`manager-row mcp-call-log-row ${resultClass(entry)}`}
                    key={entry.id}
                    role="row"
                    tabIndex={0}
                    title={t.mcpCallOpenDetail}
                    onClick={() => select(entry)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        select(entry)
                      }
                    }}
                  >
                    <span className="mcp-call-log-time" role="cell" title={new Date(entry.ts).toLocaleString()}>
                      {formatClock(entry.ts)}
                    </span>
                    <span className="mcp-call-log-action" role="cell" title={entry.action}>
                      <span>{entry.action}</span>
                      <small className={`background-session-source is-${entry.source}`}>
                        {entry.source.toUpperCase()}
                      </small>
                    </span>
                    <span role="cell" title={approvalLabel(entry.approval)}>
                      <span className="mcp-call-approval-badge">{approvalLabel(entry.approval)}</span>
                    </span>
                    <span className="mcp-call-log-detail" role="cell" title={summarizeParams(entry)}>
                      {summarizeParams(entry) || '—'}
                    </span>
                    <span
                      className={`mcp-call-log-result ${resultClass(entry)}`}
                      role="cell"
                      title={entry.error ?? resultLabel(entry)}
                    >
                      <span>{resultLabel(entry)}</span>
                      <small>{entry.durationMs} ms</small>
                    </span>
                  </div>
                ))}
                {!entries.length && !loadError ? (
                  <div className="background-session-empty">
                    <AppIcon name="call-log" size={24} />
                    <strong>{initialLoading ? t.mcpCallRecordsLoading : t.mcpCallRecordsEmpty}</strong>
                    {!initialLoading ? <span>{t.mcpCallRecordsEmptyHint}</span> : null}
                  </div>
                ) : null}
                {page?.hasMore && page.nextCursor ? (
                  <div className="mcp-call-log-more" role="row">
                    <span role="cell" aria-colspan={5}>
                      <Button loading={loading} size="sm" onClick={() => void load(page.nextCursor!)}>
                        {t.mcpCallLoadMore}
                      </Button>
                    </span>
                  </div>
                ) : null}
              </div>
              <VerticalScrollbar ariaLabel={t.mcpCallRecordsTitle} scrollRef={bodyRef} />
            </div>
          </div>
        </section>
      </div>

      {selected ? (
        <ConfirmActionDialog
          cancelLabel={t.cancel}
          className="mcp-call-log-dialog"
          confirmLabel={t.mcpCallCopyJson}
          confirmVariant="primary"
          errorMessage={copyError}
          isSubmitting={copying}
          onClose={() => setSelected(null)}
          onConfirm={async () => {
            setCopying(true)
            setCopyError(null)
            try {
              await writeClipboardText(JSON.stringify(selected, null, 2))
              setSelected(null)
            } catch (error) {
              setCopyError(error instanceof Error ? error.message : String(error))
            } finally {
              setCopying(false)
            }
          }}
          title={t.mcpCallDetailTitle}
          description={
            <div className="mcp-call-log-detail-scroll">
              <div className="mcp-call-log-detail-body" ref={detailRef}>
                <div className="mcp-call-log-detail-grid">
                  <span>{t.mcpCallColumnTime}</span>
                  <span>{new Date(selected.ts).toLocaleString()}</span>
                  <span>{t.mcpCallColumnAction}</span>
                  <span>{selected.action}</span>
                  <span>{t.mcpCallColumnApproval}</span>
                  <span>{approvalLabel(selected.approval)}</span>
                  <span>{t.mcpCallDetailConnection}</span>
                  <span>{summarizeParams(selected) || selected.target?.tabId || '—'}</span>
                  <span>{t.mcpCallDetailBridgeSession}</span>
                  <span>{selected.bridgeSessionId ?? '—'}</span>
                  <span>{t.mcpCallColumnResult}</span>
                  <span>
                    {resultLabel(selected)} · {selected.durationMs} ms
                  </span>
                  {selected.execution?.exitCode != null ? (
                    <>
                      <span>{t.mcpCallDetailExitCode}</span>
                      <span>{selected.execution.exitCode}</span>
                    </>
                  ) : null}
                </div>
                {selected.error ? <p className="mcp-call-log-error">{selected.error}</p> : null}
                <p className="mcp-call-log-metadata-hint">{t.mcpCallMetadataHint}</p>
                <pre>{JSON.stringify(selected.params, null, 2)}</pre>
              </div>
              <VerticalScrollbar ariaLabel={t.mcpCallDetailTitle} scrollRef={detailRef} />
            </div>
          }
        />
      ) : null}
    </section>
  )
}
