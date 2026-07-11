import { useState, useEffect, type DragEvent, FormEvent, MouseEvent, ReactNode, RefObject } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { getDisplayFileIconName, getDisplayFileTypeLabel } from './file-kind'

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
        aria-label={`${label}路径`}
        disabled={disabled}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {action}
      {hint ? <span>{hint}</span> : null}
    </form>
  )
}

export function FileTable({
  scrollRef,
  rows,
  compact = false,
  emptyText,
  sortState,
  cutPaths,
  selectedPaths,
  onClearSelection,
  onContextItem,
  onDragItem,
  onOpenItem,
  onSelectItem,
  onToggleSort,
  onSelectionDragEnter,
  onSelectionDragStart,
  resetColumnsTrigger
}: {
  scrollRef: RefObject<HTMLDivElement | null>
  rows: RemoteFileItem[]
  compact?: boolean
  emptyText?: string
  sortState?: RemoteFileSortState
  cutPaths?: string[]
  selectedPaths?: string[]
  onClearSelection?(): void
  onContextItem?(event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem): void
  onDragItem?(event: DragEvent<HTMLElement>, item: RemoteFileItem): void
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

  const [widths, setWidths] = useState<number[]>([260, 100, 100, 150, 100, 100])

  useEffect(() => {
    if (resetColumnsTrigger !== undefined && resetColumnsTrigger > 0) {
      setWidths([260, 100, 100, 150, 100, 100])
    }
  }, [resetColumnsTrigger])

  const handleMouseDown = (e: React.MouseEvent, index: number) => {
    e.preventDefault()
    e.stopPropagation()

    const thElement = e.currentTarget.parentElement
    if (!thElement) return
    const trElement = thElement.parentElement
    if (!trElement) return
    const ths = Array.from(trElement.querySelectorAll('th'))
    const currentWidths = ths.map((th) => th.offsetWidth)
    setWidths(currentWidths)

    const startX = e.clientX
    const startWidth = currentWidths[index]

    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    document.body.style.setProperty('-webkit-user-select', 'none')

    const handleMouseMove = (moveEvent: globalThis.MouseEvent) => {
      const deltaX = moveEvent.clientX - startX
      const newWidth = Math.max(50, startWidth + deltaX)
      setWidths((prev) => {
        const next = [...currentWidths]
        next[index] = newWidth
        return next
      })
    }

    const handleMouseUp = () => {
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      document.body.style.removeProperty('-webkit-user-select')
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
    }

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
  }

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 33,
    overscan: 15
  })

  const virtualItems = rowVirtualizer.getVirtualItems()
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0
  const paddingBottom =
    virtualItems.length > 0 ? rowVirtualizer.getTotalSize() - virtualItems[virtualItems.length - 1].end : 0

  return (
    <table
      className={`fs-file-table ${compact ? 'compact' : ''}`}
      style={!compact ? { width: `${widths.reduce((sum, w) => sum + w, 0)}px`, minWidth: '100%' } : undefined}
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClearSelection?.()
        }
      }}
    >
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
                  if ((e.target as HTMLElement).classList.contains('col-resizer')) {
                    e.stopPropagation()
                    return
                  }
                  onToggleSort?.(header.field)
                }}
                title={onToggleSort ? `${header.label} - 单击切换排序` : undefined}
              >
                <span className={`file-table-heading ${isActive ? 'is-active' : ''}`}>
                  <span>{header.label}</span>
                  {!compact ? <span className="file-table-sort-indicator">{directionIcon}</span> : null}
                </span>
                {!compact && index < headerCells.length - 1 && (
                  <div
                    className="col-resizer"
                    onMouseDown={(e) => handleMouseDown(e, index)}
                    onClick={(e) => e.stopPropagation()}
                  />
                )}
              </th>
            )
          })}
        </tr>
      </thead>
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
                  ref={rowVirtualizer.measureElement}
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
                      draggable={row.type === 'file'}
                      onDragStart={(event) => onDragItem?.(event, row)}
                    />
                  </td>
                  {!compact ? <td>{row.size}</td> : null}
                  {!compact ? <td>{typeLabel}</td> : null}
                  {!compact ? <td>{row.modified}</td> : null}
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
  )
}

export function LocalFileTable({
  scrollRef,
  cutPaths,
  rows,
  selectedPaths,
  onClearSelection,
  onContextItem,
  onDragItem,
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
  onDragItem(event: DragEvent<HTMLElement>, item: LocalFileItem): void
  onOpenItem(item: LocalFileItem): void
  onSelectItem(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
  onSelectionDragEnter(item: LocalFileItem): void
  onSelectionDragStart(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
}) {
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 33,
    overscan: 15
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
              ref={rowVirtualizer.measureElement}
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
                  onDragStart={(event) => onDragItem(event, row)}
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
  onDragStart
}: {
  draggable: boolean
  iconName: ReturnType<typeof getDisplayFileIconName>
  item: LocalFileItem | RemoteFileItem
  onDragStart(event: DragEvent<HTMLElement>): void
}) {
  return (
    <span className="file-name-cell" title={item.name}>
      <span
        className={`file-icon ${draggable ? 'is-draggable' : ''}`}
        draggable={draggable}
        onDragStart={onDragStart}
        onMouseDown={(event) => event.stopPropagation()}
        title={draggable ? t.dragTransfer : undefined}
      >
        <AppIcon name={iconName} />
      </span>
      <span className="file-row-text">
        <span className="file-row-main">{item.name}</span>
      </span>
    </span>
  )
}
