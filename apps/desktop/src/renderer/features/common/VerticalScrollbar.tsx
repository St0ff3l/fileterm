import { useCallback, useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type RefObject } from 'react'

const MIN_THUMB_HEIGHT = 24
const AUTO_HIDE_DELAY_MS = 900

type ScrollMetrics = {
  maxScrollTop: number
  trackHeight: number
  thumbHeight: number
  thumbTop: number
}

const EMPTY_METRICS: ScrollMetrics = { maxScrollTop: 0, trackHeight: 0, thumbHeight: 0, thumbTop: 0 }

/** Reusable overlay scrollbar matching the terminal's compact xterm slider. */
export function VerticalScrollbar({
  ariaLabel = '滚动内容',
  scrollRef,
  topInset = 0
}: {
  ariaLabel?: string
  scrollRef: RefObject<HTMLElement | null>
  /** Reserves a fixed header above the scrollable content. */
  topInset?: number
}) {
  const [metrics, setMetrics] = useState<ScrollMetrics>(EMPTY_METRICS)
  const [isVisible, setIsVisible] = useState(false)
  const dragRef = useRef<{ clientY: number; thumbTop: number } | null>(null)
  const hideTimerRef = useRef<number | null>(null)

  const reveal = useCallback(() => {
    if (hideTimerRef.current) {
      window.clearTimeout(hideTimerRef.current)
    }
    setIsVisible(true)
    hideTimerRef.current = window.setTimeout(() => {
      hideTimerRef.current = null
      setIsVisible(false)
    }, AUTO_HIDE_DELAY_MS)
  }, [])

  useEffect(
    () => () => {
      if (hideTimerRef.current) {
        window.clearTimeout(hideTimerRef.current)
      }
    },
    []
  )

  const updateMetrics = useCallback(() => {
    const element = scrollRef.current
    if (!element || element.clientHeight <= 0) {
      setMetrics(EMPTY_METRICS)
      return
    }

    const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight)
    if (maxScrollTop === 0) {
      setMetrics(EMPTY_METRICS)
      return
    }

    const trackHeight = Math.max(0, element.clientHeight - topInset)
    const scrollableContentHeight = Math.max(trackHeight, element.scrollHeight - topInset)
    if (trackHeight === 0) {
      setMetrics(EMPTY_METRICS)
      return
    }

    const thumbHeight = Math.min(
      trackHeight,
      Math.max(MIN_THUMB_HEIGHT, (trackHeight * trackHeight) / scrollableContentHeight)
    )
    const maxThumbTop = trackHeight - thumbHeight
    setMetrics({
      maxScrollTop,
      trackHeight,
      thumbHeight,
      thumbTop: (element.scrollTop / maxScrollTop) * maxThumbTop
    })
  }, [scrollRef, topInset])

  useEffect(() => {
    const element = scrollRef.current
    if (!element) return

    let frame = 0
    const scheduleUpdate = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(updateMetrics)
    }
    const resizeObserver = new ResizeObserver(scheduleUpdate)
    const handleScroll = () => {
      reveal()
      scheduleUpdate()
    }

    scheduleUpdate()
    element.addEventListener('scroll', handleScroll, { passive: true })
    resizeObserver.observe(element)
    if (element.firstElementChild) resizeObserver.observe(element.firstElementChild)

    return () => {
      cancelAnimationFrame(frame)
      element.removeEventListener('scroll', handleScroll)
      resizeObserver.disconnect()
    }
  }, [reveal, scrollRef, updateMetrics])

  const setScrollFromThumbTop = (thumbTop: number) => {
    const element = scrollRef.current
    if (!element || metrics.maxScrollTop === 0) return

    const maxThumbTop = metrics.trackHeight - metrics.thumbHeight
    if (maxThumbTop <= 0) return
    element.scrollTop = (Math.max(0, Math.min(maxThumbTop, thumbTop)) / maxThumbTop) * metrics.maxScrollTop
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const element = scrollRef.current
    if (!element) return

    const page = Math.max(32, element.clientHeight - 32)
    const offsets: Record<string, number> = {
      ArrowDown: 32,
      ArrowUp: -32,
      PageDown: page,
      PageUp: -page
    }
    if (event.key === 'Home') {
      element.scrollTop = 0
    } else if (event.key === 'End') {
      element.scrollTop = element.scrollHeight
    } else if (event.key in offsets) {
      element.scrollBy({ top: offsets[event.key] })
    } else {
      return
    }
    event.preventDefault()
  }

  if (metrics.maxScrollTop === 0) return null

  return (
    <div
      aria-label={ariaLabel}
      aria-orientation="vertical"
      aria-valuemax={Math.round(metrics.maxScrollTop)}
      aria-valuemin={0}
      aria-valuenow={Math.round(scrollRef.current?.scrollTop ?? 0)}
      className={`vertical-scrollbar${isVisible ? ' is-visible' : ''}`}
      onBlur={() => {
        if (!dragRef.current) reveal()
      }}
      onFocus={reveal}
      onKeyDown={handleKeyDown}
      onPointerEnter={reveal}
      onPointerLeave={() => {
        if (!dragRef.current) reveal()
      }}
      onPointerDown={(event) => {
        if (event.target !== event.currentTarget) return
        reveal()
        const rect = event.currentTarget.getBoundingClientRect()
        setScrollFromThumbTop(event.clientY - rect.top - metrics.thumbHeight / 2)
        event.preventDefault()
      }}
      role="scrollbar"
      style={{ '--vertical-scrollbar-inset-top': `${topInset}px` } as CSSProperties}
      tabIndex={0}
    >
      <div
        className="vertical-scrollbar__thumb"
        onPointerDown={(event) => {
          reveal()
          dragRef.current = { clientY: event.clientY, thumbTop: metrics.thumbTop }
          event.currentTarget.setPointerCapture(event.pointerId)
          event.stopPropagation()
          event.preventDefault()
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current
          if (!drag) return
          reveal()
          setScrollFromThumbTop(drag.thumbTop + event.clientY - drag.clientY)
        }}
        onPointerUp={(event) => {
          dragRef.current = null
          event.currentTarget.releasePointerCapture(event.pointerId)
          reveal()
        }}
        style={{ height: `${metrics.thumbHeight}px`, transform: `translateY(${metrics.thumbTop}px)` }}
      />
    </div>
  )
}
