import { useCallback, useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type RefObject } from 'react'
import { t } from '../../i18n'

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
  ariaLabel = t.scrollContent,
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
  const isVisibleRef = useRef(false)
  const metricsRef = useRef<ScrollMetrics>(EMPTY_METRICS)
  const scrollbarRef = useRef<HTMLDivElement | null>(null)
  const thumbRef = useRef<HTMLDivElement | null>(null)

  const reveal = useCallback(() => {
    if (hideTimerRef.current !== null) {
      window.clearTimeout(hideTimerRef.current)
    }
    if (!isVisibleRef.current) {
      isVisibleRef.current = true
      setIsVisible(true)
    }
    hideTimerRef.current = window.setTimeout(() => {
      hideTimerRef.current = null
      isVisibleRef.current = false
      setIsVisible(false)
    }, AUTO_HIDE_DELAY_MS)
  }, [])

  useEffect(
    () => () => {
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current)
      }
    },
    []
  )

  const publishMetrics = useCallback(
    (next: ScrollMetrics) => {
      const previous = metricsRef.current
      metricsRef.current = next

      const thumb = thumbRef.current
      if (thumb) {
        thumb.style.height = `${next.thumbHeight}px`
        thumb.style.transform = `translateY(${next.thumbTop}px)`
      }

      const scrollbar = scrollbarRef.current
      if (scrollbar) {
        scrollbar.setAttribute('aria-valuenow', `${Math.round(scrollRef.current?.scrollTop ?? 0)}`)
      }

      const staticMetricsChanged =
        previous.maxScrollTop !== next.maxScrollTop ||
        previous.trackHeight !== next.trackHeight ||
        previous.thumbHeight !== next.thumbHeight
      if (staticMetricsChanged) {
        setMetrics(next)
      }
    },
    [scrollRef]
  )

  const updateMetrics = useCallback(() => {
    const element = scrollRef.current
    if (!element || element.clientHeight <= 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight)
    if (maxScrollTop === 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const trackHeight = Math.max(0, element.clientHeight - topInset)
    const scrollableContentHeight = Math.max(trackHeight, element.scrollHeight - topInset)
    if (trackHeight === 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const thumbHeight = Math.min(
      trackHeight,
      Math.max(MIN_THUMB_HEIGHT, (trackHeight * trackHeight) / scrollableContentHeight)
    )
    const maxThumbTop = trackHeight - thumbHeight
    const scrollTop = Math.max(0, Math.min(maxScrollTop, element.scrollTop))
    publishMetrics({
      maxScrollTop,
      trackHeight,
      thumbHeight,
      thumbTop: (scrollTop / maxScrollTop) * maxThumbTop
    })
  }, [publishMetrics, scrollRef, topInset])

  const updateThumbPosition = useCallback(() => {
    const element = scrollRef.current
    const current = metricsRef.current
    if (!element || current.maxScrollTop === 0) return

    const maxThumbTop = current.trackHeight - current.thumbHeight
    if (maxThumbTop <= 0) return

    const scrollTop = Math.max(0, Math.min(current.maxScrollTop, element.scrollTop))
    publishMetrics({
      ...current,
      thumbTop: (scrollTop / current.maxScrollTop) * maxThumbTop
    })
  }, [publishMetrics, scrollRef])

  useEffect(() => {
    const element = scrollRef.current
    if (!element) return

    let frame = 0
    const scheduleMetricsUpdate = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(updateMetrics)
    }
    const scheduleThumbUpdate = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(updateThumbPosition)
    }
    const resizeObserver = new ResizeObserver(scheduleMetricsUpdate)
    const handleScroll = () => {
      reveal()
      scheduleThumbUpdate()
    }

    scheduleMetricsUpdate()
    element.addEventListener('scroll', handleScroll, { passive: true })
    resizeObserver.observe(element)
    if (element.firstElementChild) resizeObserver.observe(element.firstElementChild)

    return () => {
      cancelAnimationFrame(frame)
      element.removeEventListener('scroll', handleScroll)
      resizeObserver.disconnect()
    }
  }, [reveal, scrollRef, updateMetrics, updateThumbPosition])

  const setScrollFromThumbTop = (thumbTop: number) => {
    const element = scrollRef.current
    const current = metricsRef.current
    if (!element || current.maxScrollTop === 0) return

    const maxThumbTop = current.trackHeight - current.thumbHeight
    if (maxThumbTop <= 0) return
    element.scrollTop = (Math.max(0, Math.min(maxThumbTop, thumbTop)) / maxThumbTop) * current.maxScrollTop
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

  const renderedMetrics = metricsRef.current

  return (
    <div
      aria-label={ariaLabel}
      aria-orientation="vertical"
      aria-valuemax={Math.round(renderedMetrics.maxScrollTop)}
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
        setScrollFromThumbTop(event.clientY - rect.top - metricsRef.current.thumbHeight / 2)
        event.preventDefault()
      }}
      role="scrollbar"
      ref={scrollbarRef}
      style={{ '--vertical-scrollbar-inset-top': `${topInset}px` } as CSSProperties}
      tabIndex={0}
    >
      <div
        className="vertical-scrollbar__thumb"
        onPointerDown={(event) => {
          reveal()
          dragRef.current = { clientY: event.clientY, thumbTop: metricsRef.current.thumbTop }
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
        ref={thumbRef}
        style={{ height: `${renderedMetrics.thumbHeight}px`, transform: `translateY(${renderedMetrics.thumbTop}px)` }}
      />
    </div>
  )
}
