import { useCallback, useEffect, useState } from 'react'
import type { McpCallLogEntry, McpCallLogPage } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../../components/common/confirm-action-dialog/confirm-action-dialog'
import './mcp-call-log.css'

const PAGE_SIZE = 60

function approvalBadgeClass(approval: McpCallLogEntry['approval']): string {
  switch (approval) {
    case 'connection-trust':
      return 'is-trust'
    case 'user':
      return 'is-user'
    case 'blocked-by-policy':
      return 'is-blocked'
    default:
      return 'is-passive'
  }
}

function approvalLabel(approval: McpCallLogEntry['approval']): string {
  switch (approval) {
    case 'connection-trust':
      return t.mcpCallApprovalTrust
    case 'user':
      return t.mcpCallApprovalUser
    case 'blocked-by-policy':
      return t.mcpCallApprovalBlocked
    case 'not-required':
      return t.mcpCallApprovalPassive
    default:
      return approval
  }
}

function summarizeParams(entry: McpCallLogEntry): string {
  const params = entry.params
  if (!params || typeof params !== 'object') {
    return ''
  }
  const record = params as Record<string, unknown>
  const parts: string[] = []
  for (const key of ['command', 'profile_id', 'path', 'target_path', 'remote_path', 'name']) {
    const value = record[key]
    if (typeof value === 'string' && value.trim()) {
      parts.push(value.trim().replace(/\s+/g, ' '))
    }
  }
  if (!parts.length) {
    for (const value of Object.values(record)) {
      if (typeof value === 'string' && value.trim()) {
        parts.push(value.trim())
      }
      if (parts.length >= 1) {
        break
      }
    }
  }
  const summary = parts.join(' · ')
  return summary.length > 120 ? `${summary.slice(0, 117)}…` : summary
}

function formatClock(ts: string): string {
  // ts is RFC3339; show the local time portion in a compact form.
  const date = new Date(ts)
  if (Number.isNaN(date.getTime())) {
    return ts
  }
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

export function McpCallLogPage() {
  const [page, setPage] = useState<McpCallLogPage | null>(null)
  const [loading, setLoading] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [selected, setSelected] = useState<McpCallLogEntry | null>(null)

  const load = useCallback(async (offset: number, append: boolean) => {
    setLoading(true)
    setLoadError(null)
    try {
      const next = await window.fileterm?.readMcpCallLog(PAGE_SIZE, offset)
      if (!next) {
        throw new Error('FileTerm desktop API is unavailable')
      }
      setPage((previous) => {
        if (append && previous) {
          return {
            ...next,
            items: [...previous.items, ...next.items],
            total: next.total
          }
        }
        return next
      })
    } catch (error) {
      setLoadError(String(error instanceof Error ? error.message : error))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load(0, false)
  }, [load])

  const entries = page?.items ?? []

  return (
    <section className="background-sessions-page manager-inline connection-manager-modal mcp-call-log-page">
      <div className="connection-manager-header">
        <div className="mcp-call-log-heading">
          <span className="connection-manager-title">
            <AppIcon name="shield" size={20} />
            <span>{t.mcpCallRecordsTitle}</span>
          </span>
          <p className="mcp-call-log-subtitle">{t.mcpCallRecordsDescription}</p>
        </div>
        <button
          className="manager-icon-action"
          disabled={loading}
          title={t.refresh}
          type="button"
          onClick={() => void load(0, false)}
        >
          <AppIcon name="refresh" size={16} strokeWidth={2} />
        </button>
      </div>

      {loadError ? <p className="mcp-call-log-error">{loadError}</p> : null}

      <div className="manager-table mcp-call-log-table" role="table">
        <div className="manager-head" role="row">
          <span role="columnheader">{t.mcpCallColumnTime}</span>
          <span role="columnheader">{t.mcpCallColumnAction}</span>
          <span role="columnheader">{t.mcpCallColumnApproval}</span>
          <span role="columnheader">{t.mcpCallColumnDetail}</span>
          <span role="columnheader">{t.mcpCallColumnResult}</span>
        </div>
        <div className="manager-body" role="rowgroup">
          {entries.map((entry, index) => (
            <div
              className={`manager-row mcp-call-log-row ${entry.ok ? '' : 'is-failed'}`}
              key={`${entry.ts}-${index}`}
              role="row"
              tabIndex={0}
              title={t.mcpCallOpenDetail}
              onClick={() => setSelected(entry)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault()
                  setSelected(entry)
                }
              }}
            >
              <span className="mcp-call-log-time" role="cell">
                {formatClock(entry.ts)}
              </span>
              <span className="mcp-call-log-action" role="cell">
                {entry.action}
                {entry.source === 'cli' ? <small className="mcp-call-log-source">CLI</small> : null}
              </span>
              <span role="cell">
                <span className={`mcp-call-approval-badge ${approvalBadgeClass(entry.approval)}`}>
                  {approvalLabel(entry.approval)}
                </span>
              </span>
              <span className="mcp-call-log-detail" role="cell" title={summarizeParams(entry)}>
                {summarizeParams(entry) || '—'}
              </span>
              <span className="mcp-call-log-result" role="cell">
                {entry.ok ? (
                  <span className="mcp-call-log-duration">{entry.durationMs} ms</span>
                ) : (
                  <span className="mcp-call-log-error-inline" title={entry.error ?? undefined}>
                    {entry.error ?? t.mcpCallResultFailed}
                  </span>
                )}
              </span>
            </div>
          ))}
          {!entries.length && !loading ? (
            <div className="mcp-call-log-empty">
              <p>{t.mcpCallRecordsEmpty}</p>
              <small>{t.mcpCallRecordsEmptyHint}</small>
            </div>
          ) : null}
          {loading && !entries.length ? (
            <div className="mcp-call-log-empty">
              <p>{t.mcpCallRecordsLoading}</p>
            </div>
          ) : null}
        </div>
      </div>

      {selected ? (
        <ConfirmActionDialog
          cancelLabel={t.cancel}
          className="mcp-call-log-dialog"
          confirmLabel={t.mcpCallCopyJson}
          confirmVariant="primary"
          initialFocus="none"
          onClose={() => setSelected(null)}
          onConfirm={() => {
            void navigator.clipboard?.writeText(JSON.stringify(selected, null, 2))
            setSelected(null)
          }}
          title={
            <span className="mcp-call-log-dialog-title">
              <span>{t.mcpCallDetailTitle}</span>
              <span className={`mcp-call-approval-badge ${approvalBadgeClass(selected.approval)}`}>
                {approvalLabel(selected.approval)}
              </span>
            </span>
          }
          description={
            <div className="mcp-call-log-detail-body">
              <div className="mcp-call-log-detail-grid">
                <span>{t.mcpCallColumnTime}</span>
                <span>{formatClock(selected.ts)}</span>
                <span>{t.mcpCallColumnAction}</span>
                <span>{selected.action}</span>
                <span>{t.mcpCallDetailConnection}</span>
                <span>{selected.clientId ?? '—'}</span>
                <span>{t.mcpCallColumnResult}</span>
                <span>{selected.ok ? `${t.mcpCallDetailOk} · ${selected.durationMs} ms` : t.mcpCallResultFailed}</span>
              </div>
              {selected.error ? <pre className="mcp-call-log-detail-error">{selected.error}</pre> : null}
              <pre className="mcp-call-log-detail-json">{JSON.stringify(selected.params ?? {}, null, 2)}</pre>
            </div>
          }
        />
      ) : null}

      <div className="mcp-call-log-footer">
        <small>{formatMessageCount(page?.total ?? 0)}</small>
        {page?.hasMore ? (
          <button
            className="mcp-call-log-more"
            disabled={loading}
            type="button"
            onClick={() => void load(entries.length, true)}
          >
            {t.mcpCallLoadMore}
          </button>
        ) : null}
      </div>
    </section>
  )
}

function formatMessageCount(total: number): string {
  return t.mcpCallRecordsCount.replace('{count}', String(total))
}
