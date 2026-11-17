import { useMemo, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { ReactNode } from 'react'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { TableColumnResizer } from '../../components/common/table-column-resizer/table-column-resizer'
import { useRememberedColumnWidths } from '../common/table-column-layout/table-column-layout'
import { AppIcon } from '../common/app-icon'
import { sortNetworkRows, type NetworkTableSort } from './network-connections-model'
import { MetricHoverDetail } from './system-resource-meters'

export function NetworkSocketTable({
  headers,
  layoutId,
  rows: sourceRows,
  sort,
  onSortChange,
  empty
}: {
  headers: string[]
  layoutId: string
  rows: Array<{
    key: string
    cells: ReactNode[]
    sortValues: Array<string | number | null>
    selected?: boolean
    onSelect?(): void
  }>
  sort: NetworkTableSort
  onSortChange(sort: NetworkTableSort): void
  empty: string
}) {
  const rows = useMemo(() => sortNetworkRows(sourceRows, sort), [sourceRows, sort])
  const [widths, setWidths, commitWidths] = useRememberedColumnWidths(layoutId, headers.length)
  const scrollRef = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 24,
    getItemKey: (index) => rows[index].key,
    overscan: 8
  })
  const items = virtualizer.getVirtualItems()
  return (
    <div className="network-sockets-table-region">
      <div className="network-sockets-scroll" ref={scrollRef}>
        <table
          className="network-sockets-table"
          style={{
            width: widths ? `max(100%, ${widths.reduce((sum, width) => sum + width, 0)}px)` : '100%',
            minWidth: widths ? undefined : headers.length * 110
          }}
        >
          {widths ? (
            <colgroup>
              {widths.map((width, index) => (
                <col key={index} style={{ width }} />
              ))}
            </colgroup>
          ) : null}
          <thead>
            <tr>
              {headers.map((header, index) => (
                <th
                  key={header}
                  scope="col"
                  aria-sort={sort.column === index ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'}
                >
                  <button
                    className="network-sockets-sort"
                    type="button"
                    onClick={() =>
                      onSortChange({
                        column: index,
                        direction: sort.column === index && sort.direction === 'asc' ? 'desc' : 'asc'
                      })
                    }
                  >
                    <span className="table-column-heading">{header}</span>
                    {sort.column === index ? (
                      <AppIcon name={sort.direction === 'asc' ? 'arrow-up' : 'arrow-down'} size={11} />
                    ) : null}
                  </button>
                  {index > 0 ? (
                    <TableColumnResizer label={headers[index - 1]} onResize={setWidths} onResizeEnd={commitWidths} />
                  ) : null}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {items.length > 0 && (
              <tr aria-hidden="true">
                <td colSpan={headers.length} style={{ height: items[0].start, padding: 0 }} />
              </tr>
            )}
            {items.map((item) => {
              const row = rows[item.index]
              return (
                <tr
                  key={row.key}
                  className={`network-socket-row ${item.index % 2 ? 'is-alternate' : ''} ${row.selected ? 'is-selected' : ''}`}
                  onClick={row.onSelect}
                  aria-selected={row.onSelect ? Boolean(row.selected) : undefined}
                >
                  {row.cells.map((cell, i) => (
                    <td key={i}>
                      {typeof cell === 'string' ? (
                        <MetricHoverDetail className="network-socket-hover-detail" value={cell} />
                      ) : (
                        cell
                      )}
                    </td>
                  ))}
                </tr>
              )
            })}
            {items.length > 0 && (
              <tr aria-hidden="true">
                <td
                  colSpan={headers.length}
                  style={{ height: virtualizer.getTotalSize() - items[items.length - 1].end, padding: 0 }}
                />
              </tr>
            )}
            {!rows.length && (
              <tr>
                <td colSpan={headers.length} className="network-sockets-empty">
                  {empty}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <VerticalScrollbar scrollRef={scrollRef} />
    </div>
  )
}
