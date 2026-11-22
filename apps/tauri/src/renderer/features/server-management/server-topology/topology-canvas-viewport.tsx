import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode
} from 'react'
import { Button } from '../../../components/common/button/button'
import { MaterialIcon } from '../../../components/common/material-icon/material-icon'
import { t } from '../../../i18n'
import { VerticalScrollbar } from '../../common/vertical-scrollbar'

const MIN_ZOOM = 0.35
const MAX_ZOOM = 2.5
const PAN_THRESHOLD = 3

type CanvasPan = {
  pointerId: number
  clientX: number
  clientY: number
  scrollLeft: number
  scrollTop: number
  moved: boolean
}

type TopologyCanvasViewportProps = {
  width: number
  height: number
  ariaLabel: string
  scrollRef: { current: HTMLDivElement | null }
  children(zoom: number): ReactNode
}

export function TopologyCanvasViewport({ width, height, ariaLabel, scrollRef, children }: TopologyCanvasViewportProps) {
  const [zoom, setZoom] = useState(1)
  const [panning, setPanning] = useState(false)
  const viewportRef = useRef<HTMLDivElement>(null)
  const zoomRef = useRef(1)
  const gestureStartZoomRef = useRef<number | null>(null)
  const zoomFrameRef = useRef<number | null>(null)
  const pendingZoomScrollRef = useRef<{ left: number; top: number } | null>(null)
  const spacePressedRef = useRef(false)
  const panRef = useRef<CanvasPan | null>(null)
  const suppressClickRef = useRef(false)

  const syncGridPosition = useCallback(() => {
    const scroll = scrollRef.current
    const viewport = viewportRef.current
    if (!scroll || !viewport) return
    viewport.style.setProperty('--topology-grid-offset-x', `${-scroll.scrollLeft}px`)
    viewport.style.setProperty('--topology-grid-offset-y', `${-scroll.scrollTop}px`)
  }, [scrollRef])

  useEffect(() => {
    const releaseSpace = (event: globalThis.KeyboardEvent) => {
      if (event.code === 'Space') spacePressedRef.current = false
    }
    const resetSpace = () => {
      spacePressedRef.current = false
      panRef.current = null
      setPanning(false)
    }
    window.addEventListener('keyup', releaseSpace)
    window.addEventListener('blur', resetSpace)
    return () => {
      window.removeEventListener('keyup', releaseSpace)
      window.removeEventListener('blur', resetSpace)
    }
  }, [])

  const queueScroll = useCallback(
    (left: number, top: number) => {
      pendingZoomScrollRef.current = { left, top }
      if (zoomFrameRef.current !== null) return
      zoomFrameRef.current = requestAnimationFrame(() => {
        const scroll = scrollRef.current
        const pending = pendingZoomScrollRef.current
        if (scroll && pending) {
          scroll.scrollLeft = pending.left
          scroll.scrollTop = pending.top
          syncGridPosition()
        }
        pendingZoomScrollRef.current = null
        zoomFrameRef.current = null
      })
    },
    [scrollRef, syncGridPosition]
  )

  const clampZoom = (value: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value))

  const zoomAt = useCallback(
    (value: number, clientX?: number, clientY?: number) => {
      const scroll = scrollRef.current
      if (!scroll) return
      const nextZoom = Math.round(clampZoom(value) * 1000) / 1000
      if (Math.abs(nextZoom - zoomRef.current) < 0.001) return

      const bounds = scroll.getBoundingClientRect()
      const anchorX = clientX === undefined ? scroll.clientWidth / 2 : clientX - bounds.left
      const anchorY = clientY === undefined ? scroll.clientHeight / 2 : clientY - bounds.top
      const currentScroll = pendingZoomScrollRef.current ?? { left: scroll.scrollLeft, top: scroll.scrollTop }
      const worldX = (currentScroll.left + anchorX) / zoomRef.current
      const worldY = (currentScroll.top + anchorY) / zoomRef.current
      zoomRef.current = nextZoom
      setZoom(nextZoom)
      queueScroll(worldX * nextZoom - anchorX, worldY * nextZoom - anchorY)
    },
    [queueScroll, scrollRef, setZoom]
  )

  useEffect(() => {
    if (window.fileterm?.platform !== 'darwin') return
    const scroll = scrollRef.current
    if (!scroll) return

    const handleGesture = (event: Event) => {
      const gesture = event as Event & { scale: number; clientX?: number; clientY?: number }
      if (gesture.type === 'gesturestart') {
        gestureStartZoomRef.current = zoomRef.current
        event.preventDefault()
        return
      }
      if (gesture.type === 'gesturechange' && gestureStartZoomRef.current !== null) {
        event.preventDefault()
        zoomAt(gestureStartZoomRef.current * gesture.scale, gesture.clientX, gesture.clientY)
        return
      }
      if (gesture.type === 'gestureend') gestureStartZoomRef.current = null
    }

    const options: AddEventListenerOptions = { capture: true, passive: false }
    scroll.addEventListener('gesturestart', handleGesture, options)
    scroll.addEventListener('gesturechange', handleGesture, options)
    scroll.addEventListener('gestureend', handleGesture, options)
    return () => {
      scroll.removeEventListener('gesturestart', handleGesture, options)
      scroll.removeEventListener('gesturechange', handleGesture, options)
      scroll.removeEventListener('gestureend', handleGesture, options)
    }
  }, [zoomAt])

  const fitView = () => {
    const scroll = scrollRef.current
    if (!scroll) return
    const padding = 40
    const nextZoom = clampZoom(
      Math.min((scroll.clientWidth - padding) / width, (scroll.clientHeight - padding) / height)
    )
    zoomRef.current = nextZoom
    setZoom(nextZoom)
    queueScroll(
      Math.max(0, (width * nextZoom - scroll.clientWidth) / 2),
      Math.max(0, (height * nextZoom - scroll.clientHeight) / 2)
    )
  }

  const handleWheel = useCallback(
    (event: globalThis.WheelEvent) => {
      const platform = window.fileterm?.platform ?? 'browser'
      const isZoomGesture = platform === 'darwin' ? event.metaKey || event.ctrlKey : event.ctrlKey
      if (isZoomGesture) {
        event.preventDefault()
        const scroll = scrollRef.current
        const delta =
          event.deltaMode === 1
            ? event.deltaY * 16
            : event.deltaMode === 2
              ? event.deltaY * (scroll?.clientHeight ?? 800)
              : event.deltaY
        zoomAt(zoomRef.current * Math.exp(-delta * 0.002), event.clientX, event.clientY)
        return
      }
      if (event.shiftKey && scrollRef.current) {
        event.preventDefault()
        scrollRef.current.scrollLeft += event.deltaY || event.deltaX
      }
    },
    [scrollRef, zoomAt]
  )

  useEffect(() => {
    const scroll = scrollRef.current
    if (!scroll) return
    scroll.addEventListener('wheel', handleWheel, { passive: false })
    return () => scroll.removeEventListener('wheel', handleWheel)
  }, [handleWheel, scrollRef])

  const startPan = (event: PointerEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element ? event.target : null
    const interactive = target?.closest('button, input, select, textarea, [role="button"], [data-node-id]')
    const bySpace = event.button === 0 && spacePressedRef.current && !interactive
    const byMiddleMouse = event.button === 1 && !target?.closest('.server-topology__zoom-controls')
    if (!bySpace && !byMiddleMouse) {
      if (event.button === 0 && !interactive) event.currentTarget.focus({ preventScroll: true })
      return
    }
    event.preventDefault()
    const scroll = scrollRef.current
    if (!scroll) return
    event.currentTarget.setPointerCapture(event.pointerId)
    panRef.current = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      scrollLeft: scroll.scrollLeft,
      scrollTop: scroll.scrollTop,
      moved: false
    }
    setPanning(true)
  }

  const movePan = (event: PointerEvent<HTMLDivElement>) => {
    const pan = panRef.current
    const scroll = scrollRef.current
    if (!pan || pan.pointerId !== event.pointerId || !scroll) return
    const dx = event.clientX - pan.clientX
    const dy = event.clientY - pan.clientY
    pan.moved ||= Math.hypot(dx, dy) >= PAN_THRESHOLD
    scroll.scrollLeft = pan.scrollLeft - dx
    scroll.scrollTop = pan.scrollTop - dy
  }

  const endPan = (event: PointerEvent<HTMLDivElement>) => {
    const pan = panRef.current
    if (!pan || pan.pointerId !== event.pointerId) return
    panRef.current = null
    suppressClickRef.current = event.type === 'pointerup' && event.button === 0 && pan.moved
    setPanning(false)
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element ? event.target : null
    if (event.code === 'Space' && !target?.closest('button, input, select, textarea, [role="button"]')) {
      spacePressedRef.current = true
      event.preventDefault()
      return
    }
    if (target?.closest('button, input, select, textarea, [contenteditable="true"]')) return
    const isMac = window.fileterm?.platform === 'darwin'
    const hasZoomModifier = isMac ? event.metaKey : event.ctrlKey
    if (!hasZoomModifier || !['+', '=', '-', '0'].includes(event.key)) return
    event.preventDefault()
    if (event.key === '0') zoomAt(1)
    else zoomAt(zoomRef.current * (event.key === '-' ? 1 / 1.2 : 1.2))
  }

  return (
    <div
      aria-label={ariaLabel}
      className={`server-topology__viewport${panning ? ' is-panning' : ''}`}
      style={
        {
          '--topology-grid-size': `${16 * zoom}px`,
          '--topology-dot-size': `${Math.max(1, zoom)}px`
        } as CSSProperties
      }
      onClickCapture={(event) => {
        if (!suppressClickRef.current) return
        suppressClickRef.current = false
        event.preventDefault()
        event.stopPropagation()
      }}
      onKeyDown={handleKeyDown}
      onPointerCancel={endPan}
      onPointerDown={startPan}
      onPointerMove={movePan}
      onPointerUp={endPan}
      ref={viewportRef}
      role="region"
      tabIndex={0}
    >
      <div className="server-topology__scroll" onScroll={syncGridPosition} ref={scrollRef}>
        <div className="server-topology__canvas-extent" style={{ width: width * zoom, height: height * zoom }}>
          {children(zoom)}
        </div>
      </div>
      <div
        aria-description={
          window.fileterm?.platform === 'darwin'
            ? t.serverTopologyCanvasGestureHintMac
            : t.serverTopologyCanvasGestureHint
        }
        aria-label={t.serverTopologyZoomControls}
        className="server-topology__zoom-controls"
        role="group"
        title={
          window.fileterm?.platform === 'darwin'
            ? t.serverTopologyCanvasGestureHintMac
            : t.serverTopologyCanvasGestureHint
        }
      >
        <Button
          aria-label={t.serverTopologyResetZoom}
          className="server-topology__zoom-level"
          onClick={() => zoomAt(1)}
          size="compact"
          title={t.serverTopologyResetZoom}
        >
          {Math.round(zoom * 100)}%
        </Button>
        <Button
          aria-label={t.serverTopologyZoomOut}
          className="server-topology__zoom-step"
          onClick={() => zoomAt(zoomRef.current / 1.2)}
          size="compact"
          title={t.serverTopologyZoomOut}
        >
          −
        </Button>
        <Button
          aria-label={t.serverTopologyZoomIn}
          className="server-topology__zoom-step"
          onClick={() => zoomAt(zoomRef.current * 1.2)}
          size="compact"
          title={t.serverTopologyZoomIn}
        >
          +
        </Button>
        <Button
          aria-label={t.serverTopologyFitView}
          className="server-topology__zoom-fit"
          onClick={fitView}
          size="compact"
          title={t.serverTopologyFitView}
        >
          <MaterialIcon className="server-topology__zoom-fit-icon material-symbols-outlined" name="fit_screen" />
        </Button>
      </div>
      <VerticalScrollbar ariaLabel={ariaLabel} scrollRef={scrollRef} />
    </div>
  )
}
