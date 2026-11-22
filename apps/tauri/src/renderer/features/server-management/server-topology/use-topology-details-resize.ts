import { useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from 'react'

const MIN_CANVAS_WIDTH = 320
const RESIZER_WIDTH = 12
const DEFAULT_DETAILS_WIDTH = 264

type ResizeState = { pointerId: number; startX: number; startWidth: number; minWidth: number; maxWidth: number }

export function useTopologyDetailsResize() {
  const [width, setWidth] = useState<number | null>(null)
  const [isResizing, setIsResizing] = useState(false)
  const bodyRef = useRef<HTMLDivElement>(null)
  const detailsRef = useRef<HTMLElement>(null)
  const resizeRef = useRef<ResizeState | null>(null)

  const getLimits = () => {
    const body = bodyRef.current
    const details = detailsRef.current
    if (!body || !details) return null
    const minWidth = Number.parseFloat(getComputedStyle(details).minWidth) || DEFAULT_DETAILS_WIDTH
    return {
      minWidth,
      maxWidth: Math.max(minWidth, body.clientWidth - MIN_CANVAS_WIDTH - RESIZER_WIDTH)
    }
  }

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !event.isPrimary) return
    const details = detailsRef.current
    const limits = getLimits()
    if (!details || !limits) return
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    resizeRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: details.getBoundingClientRect().width,
      ...limits
    }
    setIsResizing(true)
  }

  const updateResize = (event: PointerEvent<HTMLDivElement>) => {
    const resize = resizeRef.current
    if (resize?.pointerId !== event.pointerId) return
    setWidth(Math.max(resize.minWidth, Math.min(resize.maxWidth, resize.startWidth + resize.startX - event.clientX)))
  }

  const finishResize = (event: PointerEvent<HTMLDivElement>) => {
    if (resizeRef.current?.pointerId !== event.pointerId) return
    resizeRef.current = null
    setIsResizing(false)
  }

  const resizeByKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    const limits = getLimits()
    if (!limits) return
    const currentWidth = detailsRef.current?.getBoundingClientRect().width ?? limits.minWidth
    const nextWidth =
      event.key === 'Home'
        ? limits.minWidth
        : event.key === 'End'
          ? limits.maxWidth
          : event.key === 'ArrowLeft'
            ? currentWidth + 12
            : event.key === 'ArrowRight'
              ? currentWidth - 12
              : null
    if (nextWidth === null) return
    event.preventDefault()
    setWidth(Math.max(limits.minWidth, Math.min(limits.maxWidth, nextWidth)))
  }

  const style = {
    '--server-topology-details-width': width === null ? undefined : `${width}px`
  } as CSSProperties
  const valueMax = Math.max(
    DEFAULT_DETAILS_WIDTH,
    (bodyRef.current?.clientWidth ?? DEFAULT_DETAILS_WIDTH + MIN_CANVAS_WIDTH + RESIZER_WIDTH) -
      MIN_CANVAS_WIDTH -
      RESIZER_WIDTH
  )

  return {
    bodyRef,
    detailsRef,
    isResizing,
    resizeByKeyboard,
    startResize,
    updateResize,
    finishResize,
    style,
    valueMin: DEFAULT_DETAILS_WIDTH,
    valueMax,
    valueNow: Math.round(width ?? DEFAULT_DETAILS_WIDTH)
  }
}
