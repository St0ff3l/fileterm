import { Button } from '../../components/common/button/button'
import { formatMessage, t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { SelectionControl } from '../common/selection-control'
import { StatusIndicator } from '../common/status-indicator'
import {
  nextServerListSort,
  sortServerListRows,
  type ServerListColumn,
  type ServerListRow,
  type ServerListSort
} from './server-management-list-model'
import { statusLabel } from './server-profile-status'
import './server-management-list.css'

export function ServerManagementList({
  label,
  batchEligible,
  batchLimitReached,
  checkedProfiles,
  rows,
  sort,
  selectedId,
  onSortChange,
  onCheck,
  onSelect
}: {
  label: string
  batchEligible(id: string): boolean
  batchLimitReached: boolean
  checkedProfiles: ReadonlySet<string>
  rows: ServerListRow[]
  sort: ServerListSort | null
  selectedId: string | undefined
  onSortChange(sort: ServerListSort | null): void
  onCheck(id: string, checked: boolean): void
  onSelect(id: string): void
}) {
  const headers: Array<[ServerListColumn, string]> = [
    ['name', t.name],
    ['address', t.serverDashboardColumnAddress],
    ['protocol', t.connectionType],
    ['status', t.serverDashboardColumnStatus],
    ['tags', t.serverDashboardColumnTags]
  ]
  const sorted = sortServerListRows(rows, sort)
  const showCheckboxes = rows.some(({ id }) => batchEligible(id))

  return (
    <div className="server-management-list" role="region" aria-label={label} tabIndex={0}>
      <table aria-label={label}>
        <thead>
          <tr>
            {showCheckboxes && (
              <th aria-label={t.serverBatchSelection} className="server-management-list-check" scope="col" />
            )}
            {headers.map(([column, heading]) => (
              <th
                aria-sort={column === sort?.column ? sort.direction : undefined}
                data-column={column}
                key={column}
                scope="col"
              >
                <Button
                  icon={
                    column === sort?.column ? (
                      <AppIcon name={sort.direction === 'ascending' ? 'arrow-up' : 'arrow-down'} size={12} />
                    ) : null
                  }
                  onClick={() => onSortChange(nextServerListSort(sort, column))}
                  size="panel"
                  title={t.serverDashboardSortHint}
                  variant="ghost"
                >
                  {heading}
                </Button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row) => {
            const selected = row.id === selectedId
            const checked = checkedProfiles.has(row.id)
            return (
              <tr className={selected ? 'is-selected' : undefined} key={row.id} onClick={() => onSelect(row.id)}>
                {showCheckboxes && (
                  <td className="server-management-list-check" onClick={(event) => event.stopPropagation()}>
                    {batchEligible(row.id) && (
                      <SelectionControl
                        aria-label={formatMessage(t.serverBatchSelectHost, { name: row.name })}
                        checked={checked}
                        disabled={!checked && batchLimitReached}
                        onChange={(event) => onCheck(row.id, event.target.checked)}
                        type="checkbox"
                      />
                    )}
                  </td>
                )}
                <th scope="row">
                  <Button aria-pressed={selected} size="panel" title={row.name} variant="ghost">
                    <span className="server-management-list-name">{row.name}</span>
                  </Button>
                </th>
                <td title={row.address}>{row.address}</td>
                <td>{row.protocol.toUpperCase()}</td>
                <td>
                  <span className={`server-dashboard-status is-${row.status}`}>
                    {row.status !== 'idle' ? <StatusIndicator aria-hidden="true" status={row.status} /> : null}
                    {statusLabel(row.status)}
                  </span>
                </td>
                <td title={row.tags || undefined}>{row.tags || '—'}</td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
