import { useEffect, useRef, useState, type PointerEvent } from 'react'
import type { TopologyNode } from '../server-topology-model'

const DRAG_THRESHOLD = 3

type NodeDrag = {
  pointerId: number
  clientX: number
  clientY: number
  scrollLeft: number
  scrollTop: number
  moved: boolean
  zoom: number
}

export type NodeDragPreview = { nodeId: string; dx: number; dy: number }

export function useNodeDrag(getScrollPosition: () => { left: number; top: number }) {
  const [preview, setPreview] = useState<NodeDragPreview | null>(null)
  const dragRef = useRef<NodeDrag | null>(null)
  const frameRef = useRef<number | null>(null)
  const pendingPreviewRef = useRef<NodeDragPreview | null>(null)

  useEffect(
    () => () => {
      if (frameRef.current !== null) cancelAnimationFrame(frameRef.current)
    },
    []
  )

  const startDrag = (event: PointerEvent<HTMLButtonElement>, node: TopologyNode, zoom: number) => {
    if (event.button !== 0 || !event.isPrimary || dragRef.current) return
    event.currentTarget.setPointerCapture(event.pointerId)
    const scroll = getScrollPosition()
    dragRef.current = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      scrollLeft: scroll.left,
      scrollTop: scroll.top,
      moved: false,
      zoom
    }
  }

  const updateDrag = (event: PointerEvent<HTMLButtonElement>, node: TopologyNode) => {
    const drag = dragRef.current
    if (drag?.pointerId !== event.pointerId) return
    const scroll = getScrollPosition()
    const dx = (event.clientX - drag.clientX + scroll.left - drag.scrollLeft) / drag.zoom
    const dy = (event.clientY - drag.clientY + scroll.top - drag.scrollTop) / drag.zoom
    drag.moved ||= Math.hypot(dx, dy) >= DRAG_THRESHOLD
    if (!drag.moved) return

    pendingPreviewRef.current = {
      nodeId: node.id,
      dx: Math.max(-node.x, dx),
      dy: Math.max(-node.y, dy)
    }
    if (frameRef.current !== null) return
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null
      setPreview(pendingPreviewRef.current)
      pendingPreviewRef.current = null
    })
  }

  const endDrag = (
    event: PointerEvent<HTMLButtonElement>,
    node: TopologyNode,
    commit: boolean,
    onCommit?: (node: TopologyNode, x: number, y: number) => void
  ) => {
    const drag = dragRef.current
    if (drag?.pointerId !== event.pointerId) return
    dragRef.current = null
    if (frameRef.current !== null) cancelAnimationFrame(frameRef.current)
    frameRef.current = null
    pendingPreviewRef.current = null
    setPreview(null)

    if (!commit) return
    const scroll = getScrollPosition()
    const rawDx = (event.clientX - drag.clientX + scroll.left - drag.scrollLeft) / drag.zoom
    const rawDy = (event.clientY - drag.clientY + scroll.top - drag.scrollTop) / drag.zoom
    drag.moved ||= Math.hypot(rawDx, rawDy) >= DRAG_THRESHOLD
    if (!drag.moved) return
    const dx = Math.max(-node.x, rawDx)
    const dy = Math.max(-node.y, rawDy)
    onCommit?.(node, node.x + dx, node.y + dy)
  }

  return { preview, startDrag, updateDrag, endDrag }
}
