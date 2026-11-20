import { Button } from '../../components/common/button/button'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StatusIndicator } from '../common/status-indicator'
import {
  sortServerListRows,
  type ServerListColumn,
  type ServerListRow,
  type ServerListSort
} from './server-management-list-model'
import { statusLabel } from './server-profile-status'
import './server-management-list.css'

export function ServerManagementList({
  label,
  rows,
  sort,
  selectedId,
  onSortChange,
  onSelect
}: {
  label: string
  rows: ServerListRow[]
  sort: ServerListSort
  selectedId: string | undefined
  onSortChange(sort: ServerListSort): void
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

  return (
    <div className="server-management-list" role="region" aria-label={label} tabIndex={0}>
      <table aria-label={label}>
        <thead>
          <tr>
            {headers.map(([column, heading]) => (
              <th aria-sort={column === sort.column ? sort.direction : undefined} key={column} scope="col">
                <Button
                  icon={
                    column === sort.column ? (
                      <AppIcon name={sort.direction === 'ascending' ? 'arrow-up' : 'arrow-down'} size={12} />
                    ) : null
                  }
                  onClick={() =>
                    onSortChange({
                      column,
                      direction: column === sort.column && sort.direction === 'ascending' ? 'descending' : 'ascending'
                    })
                  }
                  size="panel"
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
            return (
              <tr className={selected ? 'is-selected' : undefined} key={row.id} onClick={() => onSelect(row.id)}>
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
