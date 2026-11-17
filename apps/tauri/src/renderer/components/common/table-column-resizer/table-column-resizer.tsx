import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent } from 'react'
import { formatMessage, t } from '../../../i18n'
import './table-column-resizer.css'

const MIN_COLUMN_WIDTH = 50

type Drag = {
  widths: number[]
  index: number
  x: number
  cursor: string
  userSelect: string
  webkitUserSelect: string
  latestWidths: number[]
  commit?: (widths: number[]) => void
}

export function TableColumnResizer({
  label,
  onResize,
  onResizeEnd
}: {
  label: string
  onResize(widths: number[]): void
  onResizeEnd?(widths: number[]): void
}) {
  const ref = useRef<HTMLSpanElement>(null)
  const drag = useRef<Drag | null>(null)
  const [dragging, setDragging] = useState(false)
  const [width, setWidth] = useState(MIN_COLUMN_WIDTH)

  const stopDrag = useCallback(() => {
    const current = drag.current
    if (!current) return
    document.body.style.cursor = current.cursor
    document.body.style.userSelect = current.userSelect
    document.body.style.setProperty('-webkit-user-select', current.webkitUserSelect)
    drag.current = null
    current.commit?.(current.latestWidths)
    setDragging(false)
  }, [])

  useEffect(() => stopDrag, [stopDrag])
  useLayoutEffect(() => {
    const cell = ref.current?.closest('th')?.previousElementSibling
    if (!cell) return
    const update = () => setWidth(Math.round(cell.getBoundingClientRect().width))
    update()
    const observer = new ResizeObserver(update)
    observer.observe(cell)
    return () => observer.disconnect()
  }, [])

  const readColumns = () => {
    const cell = ref.current?.closest('th')
    if (!cell?.parentElement) return null
    const cells = Array.from(cell.parentElement.querySelectorAll('th'))
    const index = cells.indexOf(cell) - 1
    if (index < 0) return null
    return { widths: cells.map((header) => header.getBoundingClientRect().width), index }
  }

  const startDrag = (event: PointerEvent<HTMLSpanElement>) => {
    if (event.button !== 0) return
    const columns = readColumns()
    if (!columns) return
    event.preventDefault()
    event.stopPropagation()
    drag.current = {
      ...columns,
      latestWidths: columns.widths,
      commit: onResizeEnd,
      x: event.clientX,
      cursor: document.body.style.cursor,
      userSelect: document.body.style.userSelect,
      webkitUserSelect: document.body.style.getPropertyValue('-webkit-user-select')
    }
    event.currentTarget.setPointerCapture(event.pointerId)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    document.body.style.setProperty('-webkit-user-select', 'none')
    setDragging(true)
    onResize(columns.widths)
  }

  const moveDrag = (event: PointerEvent<HTMLSpanElement>) => {
    const current = drag.current
    if (!current) return
    const widths = [...current.widths]
    widths[current.index] = Math.max(MIN_COLUMN_WIDTH, widths[current.index] + event.clientX - current.x)
    current.latestWidths = widths
    onResize(widths)
  }

  const resizeWithKeyboard = (event: KeyboardEvent<HTMLSpanElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    const columns = readColumns()
    if (!columns) return
    event.preventDefault()
    event.stopPropagation()
    const step = event.shiftKey ? 20 : 10
    columns.widths[columns.index] = Math.max(
      MIN_COLUMN_WIDTH,
      columns.widths[columns.index] + (event.key === 'ArrowRight' ? step : -step)
    )
    onResize(columns.widths)
    onResizeEnd?.(columns.widths)
  }

  return (
    <span
      ref={ref}
      className="table-column-resizer"
      data-dragging={dragging || undefined}
      role="separator"
      aria-orientation="vertical"
      aria-label={formatMessage(t.resizeTableColumn, { label })}
      aria-valuemin={MIN_COLUMN_WIDTH}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={startDrag}
      onPointerMove={moveDrag}
      onPointerUp={stopDrag}
      onPointerCancel={stopDrag}
      onLostPointerCapture={stopDrag}
      onKeyDown={resizeWithKeyboard}
      onClick={(event) => event.stopPropagation()}
    />
  )
}
