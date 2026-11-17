import './file-tables.css'
import { useRememberedColumnWidths } from '../common/table-column-layout/table-column-layout'
import { TableColumnResizer } from '../../components/common/table-column-resizer/table-column-resizer'
import '../workspace/workspace-list-rows.css'
import { useCallback, useRef, useEffect, FormEvent, MouseEvent, PointerEvent, ReactNode, RefObject } from 'react'
import { createPortal } from 'react-dom'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { formatMessage, t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { getDisplayFileIconName, getDisplayFileTypeLabel } from './file-kind'
import { formatFileModified } from './file-time'

import type { FileFilterConfig } from './file-filter'

// File rows are deliberately fixed-height in workstation-skin.css. Keeping
// the virtualizer on that same value prevents it from correcting scrollTop
// while a trackpad momentum gesture is in progress.
const FILE_TABLE_ROW_HEIGHT = 24
const FILE_TABLE_OVERSCAN = 8
const DEFAULT_FILE_COLUMN_WIDTHS = [260, 100, 100, 150, 100, 100]

export type RemoteFileSortField = 'name' | 'size' | 'type' | 'modified' | 'permission' | 'ownerGroup'

export interface RemoteFileSortState {
  field: RemoteFileSortField
  direction: 'asc' | 'desc'
}

export function PanePathBar({
  hint,
  label,
  value,
  disabled = false,
  action,
  onChange,
  onSubmit
}: {
  hint?: string
  label: string
  value: string
  disabled?: boolean
  action?: ReactNode
  onChange(value: string): void
  onSubmit(event: FormEvent<HTMLFormElement>): void
}) {
  return (
    <form className="pane-path-bar" onSubmit={onSubmit}>
      <strong>{label}</strong>
      <input
        aria-label={formatMessage(t.pathAria, { label })}
        disabled={disabled}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {action}
      {hint ? <span>{hint}</span> : null}
    </form>
  )
}

export function PaneFilterBar({
  filter,
  matchCount,
  totalCount,
  disabled = false,
  onFilterChange,
  onClear
}: {
  filter: FileFilterConfig
  matchCount: number
  totalCount: number
  disabled?: boolean
  onFilterChange(nextFilter: FileFilterConfig): void
  onClear(): void
}) {
  const placeholder =
    filter.mode === 'glob'
      ? t.filterPlaceholderGlob
      : filter.mode === 'regex'
        ? t.filterPlaceholderRegex
        : t.filterPlaceholderText

  return (
    <div className={`pane-filter-bar ${disabled ? 'is-disabled' : ''}`}>
      <div className="pane-filter-input-wrapper">
        <span className="pane-filter-search-icon">
          <AppIcon name="search" size={13} />
        </span>
        <input
          aria-label={t.filterFiles}
          className="pane-filter-input"
          disabled={disabled}
          placeholder={placeholder}
          value={filter.query}
          onChange={(event) => onFilterChange({ ...filter, query: event.target.value })}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              onClear()
            }
          }}
        />
        {filter.query ? (
          <button
            aria-label={t.clearFilter}
            className="pane-filter-clear-button"
            disabled={disabled}
            type="button"
            onClick={onClear}
            title={t.clearFilter}
          >
            <AppIcon name="close" size={12} />
          </button>
        ) : null}
      </div>

      {filter.query.trim() ? (
        <span className="pane-filter-badge">
          {matchCount}/{totalCount}
        </span>
      ) : null}

      <div className="pane-filter-mode-toggles" role="group" aria-label="Filter Mode">
        <button
          className={`pane-filter-mode-btn ${filter.mode === 'text' ? 'active' : ''}`}
          disabled={disabled}
          type="button"
          onClick={() => onFilterChange({ ...filter, mode: 'text' })}
          title={t.filterModeText}
        >
          Aa
        </button>
        <button
          className={`pane-filter-mode-btn ${filter.mode === 'glob' ? 'active' : ''}`}
          disabled={disabled}
          type="button"
          onClick={() => onFilterChange({ ...filter, mode: 'glob' })}
          title={t.filterModeGlob}
        >
          *?
        </button>
        <button
          className={`pane-filter-mode-btn ${filter.mode === 'regex' ? 'active' : ''}`}
          disabled={disabled}
          type="button"
          onClick={() => onFilterChange({ ...filter, mode: 'regex' })}
          title={t.filterModeRegex}
        >
          .*
        </button>
      </div>
    </div>
  )
}

export function FileTable({
  scrollRef,
  headerContainer,
  rows,
  compact = false,
  emptyText,
  sortState,
  cutPaths,
  selectedPaths,
  onClearSelection,
  onContextItem,
  onPointerDragStart,
  onOpenItem,
  onSelectItem,
  onToggleSort,
  onSelectionDragEnter,
  onSelectionDragStart,
  resetColumnsTrigger
}: {
  scrollRef: RefObject<HTMLDivElement | null>
  headerContainer?: HTMLDivElement | null
  rows: RemoteFileItem[]
  compact?: boolean
  emptyText?: string
  sortState?: RemoteFileSortState
  cutPaths?: string[]
  selectedPaths?: string[]
  onClearSelection?(): void
  onContextItem?(event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem): void
  onPointerDragStart?(event: PointerEvent<HTMLElement>, item: RemoteFileItem): void
  onOpenItem?(item: RemoteFileItem): void
  onSelectItem?(event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem): void
  onToggleSort?(field: RemoteFileSortField): void
  onSelectionDragEnter?(item: RemoteFileItem): void
  onSelectionDragStart?(event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem): void
  resetColumnsTrigger?: number
}) {
  const headerCells: Array<{ field: RemoteFileSortField; label: string }> = [
    { field: 'name', label: t.fileName },
    { field: 'size', label: t.size },
    { field: 'type', label: t.type },
    { field: 'modified', label: t.modifiedAt },
    { field: 'permission', label: t.permission },
    { field: 'ownerGroup', label: t.ownerGroup }
  ]

  const [rememberedWidths, setWidths, commitWidths] = useRememberedColumnWidths('remote-files', 6)
  const widths = rememberedWidths ?? DEFAULT_FILE_COLUMN_WIDTHS
  const columnsResized = rememberedWidths !== null

  const lastResetColumnsTrigger = useRef(resetColumnsTrigger)
  useEffect(() => {
    if (
      resetColumnsTrigger !== lastResetColumnsTrigger.current &&
      resetColumnsTrigger !== undefined &&
      resetColumnsTrigger > 0
    ) {
      setWidths(null)
      commitWidths(null)
    }
    lastResetColumnsTrigger.current = resetColumnsTrigger
  }, [resetColumnsTrigger])

  const getItemKey = useCallback(
    (index: number) => {
      const row = rows[index]
      return row ? `${row.path}:${row.name}` : index
    },
    [rows]
  )

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => FILE_TABLE_ROW_HEIGHT,
    getItemKey,
    overscan: FILE_TABLE_OVERSCAN
  })

  const virtualItems = rowVirtualizer.getVirtualItems()
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0
  const paddingBottom =
    virtualItems.length > 0 ? rowVirtualizer.getTotalSize() - virtualItems[virtualItems.length - 1].end : 0

  const tableClass = `fs-file-table ${compact ? 'compact' : ''}`
  const tableStyle = !compact
    ? { width: `${widths.reduce((sum, w) => sum + w, 0)}px`, minWidth: columnsResized ? undefined : '100%' }
    : undefined
  const columns = !compact ? (
    <colgroup>
      {widths.map((width, index) => (
        <col key={index} style={{ width: `${width}px` }} />
      ))}
    </colgroup>
  ) : null
  const header = (
    <thead>
      <tr>
        {headerCells.map((header, index) => {
          if (compact && index > 0) {
            return null
          }

          const isActive = sortState?.field === header.field
          const directionIcon = isActive ? (
            <AppIcon name={sortState?.direction === 'asc' ? 'arrow-up' : 'arrow-down'} size={11} />
          ) : null

          return (
            <th
              key={header.field}
              className={onToggleSort ? 'is-sortable' : undefined}
              style={!compact ? { width: `${widths[index]}px` } : undefined}
              onClick={(e) => {
                if ((e.target as HTMLElement).classList.contains('table-column-resizer')) {
                  e.stopPropagation()
                  return
                }
                onToggleSort?.(header.field)
              }}
              title={onToggleSort ? formatMessage(t.sortHeaderTitle, { label: header.label }) : undefined}
            >
              <span className={`file-table-heading ${isActive ? 'is-active' : ''}`}>
                <span>{header.label}</span>
                {!compact ? <span className="file-table-sort-indicator">{directionIcon}</span> : null}
              </span>
              {!compact && index > 0 && (
                <TableColumnResizer
                  label={headerCells[index - 1].label}
                  onResize={setWidths}
                  onResizeEnd={commitWidths}
                />
              )}
            </th>
          )
        })}
      </tr>
    </thead>
  )

  return (
    <>
      {headerContainer
        ? createPortal(
            <table className={tableClass} style={tableStyle}>
              {columns}
              {header}
            </table>,
            headerContainer
          )
        : null}
      <table
        className={tableClass}
        style={tableStyle}
        onClick={(event) => {
          if (event.target === event.currentTarget) {
            onClearSelection?.()
          }
        }}
      >
        {headerContainer !== undefined ? columns : null}
        {headerContainer === undefined ? header : null}
        <tbody
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              onClearSelection?.()
            }
          }}
        >
          {rows.length ? (
            <>
              {paddingTop > 0 && (
                <tr>
                  <td colSpan={compact ? 1 : 6} style={{ height: `${paddingTop}px`, padding: 0, border: 0 }} />
                </tr>
              )}
              {virtualItems.map((virtualRow) => {
                const row = rows[virtualRow.index]
                const typeLabel = getDisplayFileTypeLabel(row)
                const iconName = getDisplayFileIconName(row)

                return (
                  <tr
                    key={row.path}
                    data-index={virtualRow.index}
                    className={`${row.type === 'folder' ? 'is-folder' : 'is-file'} ${selectedPaths?.includes(row.path) ? 'is-selected' : ''} ${cutPaths?.includes(row.path) ? 'is-cut-pending' : ''}`}
                    onClick={(event) => onSelectItem?.(event, row)}
                    onContextMenu={(event) => onContextItem?.(event, row)}
                    onDoubleClick={() => onOpenItem?.(row)}
                    onMouseDown={(event) => {
                      if (event.button === 0) {
                        onSelectionDragStart?.(event, row)
                      }
                    }}
                    onMouseEnter={() => onSelectionDragEnter?.(row)}
                  >
                    <td>
                      <FileNameCell
                        iconName={iconName}
                        item={row}
                        draggable={row.name !== '..'}
                        onPointerDown={row.name !== '..' ? (event) => onPointerDragStart?.(event, row) : undefined}
                      />
                    </td>
                    {!compact ? <td>{row.size}</td> : null}
                    {!compact ? <td>{typeLabel}</td> : null}
                    {!compact ? <td>{formatFileModified(row.modified)}</td> : null}
                    {!compact ? <td>{row.permission ?? ''}</td> : null}
                    {!compact ? <td>{row.ownerGroup ?? ''}</td> : null}
                  </tr>
                )
              })}
              {paddingBottom > 0 && (
                <tr>
                  <td colSpan={compact ? 1 : 6} style={{ height: `${paddingBottom}px`, padding: 0, border: 0 }} />
                </tr>
              )}
            </>
          ) : (
            <tr>
              <td colSpan={compact ? 1 : 6}>{emptyText ?? t.emptyFiles}</td>
            </tr>
          )}
        </tbody>
      </table>
    </>
  )
}

export function LocalFileTable({
  scrollRef,
  cutPaths,
  rows,
  selectedPaths,
  onClearSelection,
  onContextItem,
  onPointerDragStart,
  onOpenItem,
  onSelectItem,
  onSelectionDragEnter,
  onSelectionDragStart
}: {
  scrollRef: RefObject<HTMLDivElement | null>
  cutPaths?: string[]
  rows: LocalFileItem[]
  selectedPaths: string[]
  onClearSelection(): void
  onContextItem(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
  onPointerDragStart(event: PointerEvent<HTMLElement>, item: LocalFileItem): void
  onOpenItem(item: LocalFileItem): void
  onSelectItem(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
  onSelectionDragEnter(item: LocalFileItem): void
  onSelectionDragStart(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
}) {
  const getItemKey = useCallback(
    (index: number) => {
      const row = rows[index]
      return row ? `${row.path}:${row.name}` : index
    },
    [rows]
  )

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => FILE_TABLE_ROW_HEIGHT,
    getItemKey,
    overscan: FILE_TABLE_OVERSCAN
  })

  const virtualItems = rowVirtualizer.getVirtualItems()
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0
  const paddingBottom =
    virtualItems.length > 0 ? rowVirtualizer.getTotalSize() - virtualItems[virtualItems.length - 1].end : 0

  return (
    <table
      className="fs-file-table compact"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClearSelection()
        }
      }}
    >
      <thead>
        <tr>
          <th>{t.fileName}</th>
        </tr>
      </thead>
      <tbody
        onClick={(event) => {
          if (event.target === event.currentTarget) {
            onClearSelection()
          }
        }}
      >
        {paddingTop > 0 && (
          <tr>
            <td colSpan={1} style={{ height: `${paddingTop}px`, padding: 0, border: 0 }} />
          </tr>
        )}
        {virtualItems.map((virtualRow) => {
          const row = rows[virtualRow.index]
          const iconName = getDisplayFileIconName(row)

          return (
            <tr
              key={`${row.path}:${row.name}`}
              data-index={virtualRow.index}
              className={`${row.type === 'folder' ? 'is-folder' : 'is-file'} ${selectedPaths.includes(row.path) ? 'is-selected' : ''} ${cutPaths?.includes(row.path) ? 'is-cut-pending' : ''}`}
              onClick={(event) => onSelectItem(event, row)}
              onContextMenu={(event) => onContextItem(event, row)}
              onDoubleClick={() => onOpenItem(row)}
              onMouseDown={(event) => {
                if (event.button === 0) {
                  onSelectionDragStart(event, row)
                }
              }}
              onMouseEnter={() => onSelectionDragEnter(row)}
            >
              <td>
                <FileNameCell
                  iconName={iconName}
                  item={row}
                  draggable={row.name !== '..'}
                  onPointerDown={(event) => onPointerDragStart(event, row)}
                />
              </td>
            </tr>
          )
        })}
        {paddingBottom > 0 && (
          <tr>
            <td colSpan={1} style={{ height: `${paddingBottom}px`, padding: 0, border: 0 }} />
          </tr>
        )}
      </tbody>
    </table>
  )
}

function FileNameCell({
  draggable,
  iconName,
  item,
  onPointerDown
}: {
  draggable: boolean
  iconName: ReturnType<typeof getDisplayFileIconName>
  item: LocalFileItem | RemoteFileItem
  onPointerDown?(event: PointerEvent<HTMLElement>): void
}) {
  return (
    <span
      className={`file-name-cell ${draggable ? 'is-draggable' : ''}`}
      title={draggable ? `${item.name} — ${t.dragTransfer}` : item.name}
      // Keep transfers inside FileTerm; WebKit native drags cancel pointer gestures.
      draggable={false}
      onPointerDown={draggable ? onPointerDown : undefined}
      onDragStart={(event) => event.preventDefault()}
      onMouseDown={draggable ? (event) => event.stopPropagation() : undefined}
    >
      <span className="file-icon">
        <AppIcon name={iconName} />
      </span>
      <span className="file-row-text">
        <span className="file-row-main">{item.name}</span>
      </span>
    </span>
  )
}
