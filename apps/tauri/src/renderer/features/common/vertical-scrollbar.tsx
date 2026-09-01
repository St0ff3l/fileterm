import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type RefObject,
  type WheelEvent as ReactWheelEvent
} from 'react'
import { t } from '../../i18n'

const MIN_THUMB_HEIGHT = 24
const AUTO_HIDE_DELAY_MS = 900

export type VerticalScrollMetrics = {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

/**
 * Adapter for scrollable widgets whose state is not exposed through a native
 * element, such as xterm.js 6's internal scroll model.
 */
export type VerticalScrollController = {
  getElement?(): HTMLElement | null
  getMetrics(): VerticalScrollMetrics | null
  scrollTo(scrollTop: number): void
  scrollBy(delta: number): void
  subscribe(listener: () => void): () => void
}

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
  scrollController,
  topInset = 0
}: {
  ariaLabel?: string
  scrollRef?: RefObject<HTMLElement | null>
  scrollController?: VerticalScrollController | null
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

  const readScrollMetrics = useCallback((): VerticalScrollMetrics | null => {
    if (scrollController) {
      return scrollController.getMetrics()
    }

    const element = scrollRef?.current
    if (!element) {
      return null
    }

    return {
      clientHeight: element.clientHeight,
      scrollHeight: element.scrollHeight,
      scrollTop: element.scrollTop
    }
  }, [scrollController, scrollRef])

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
        scrollbar.setAttribute('aria-valuenow', `${Math.round(readScrollMetrics()?.scrollTop ?? 0)}`)
      }

      const staticMetricsChanged =
        previous.maxScrollTop !== next.maxScrollTop ||
        previous.trackHeight !== next.trackHeight ||
        previous.thumbHeight !== next.thumbHeight
      if (staticMetricsChanged) {
        setMetrics(next)
      }
    },
    [readScrollMetrics]
  )

  const updateMetrics = useCallback(() => {
    const current = readScrollMetrics()
    if (!current || current.clientHeight <= 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const maxScrollTop = Math.max(0, current.scrollHeight - current.clientHeight)
    if (maxScrollTop === 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const trackHeight = Math.max(0, current.clientHeight - topInset)
    const scrollableContentHeight = Math.max(trackHeight, current.scrollHeight - topInset)
    if (trackHeight === 0) {
      publishMetrics(EMPTY_METRICS)
      return
    }

    const thumbHeight = Math.min(
      trackHeight,
      Math.max(MIN_THUMB_HEIGHT, (trackHeight * trackHeight) / scrollableContentHeight)
    )
    const maxThumbTop = trackHeight - thumbHeight
    const scrollTop = Math.max(0, Math.min(maxScrollTop, current.scrollTop))
    publishMetrics({
      maxScrollTop,
      trackHeight,
      thumbHeight,
      thumbTop: (scrollTop / maxScrollTop) * maxThumbTop
    })
  }, [publishMetrics, readScrollMetrics, topInset])

  const updateThumbPosition = useCallback(() => {
    const currentScroll = readScrollMetrics()
    const current = metricsRef.current
    if (!currentScroll || current.maxScrollTop === 0) return

    const maxThumbTop = current.trackHeight - current.thumbHeight
    if (maxThumbTop <= 0) return

    const scrollTop = Math.max(0, Math.min(current.maxScrollTop, currentScroll.scrollTop))
    publishMetrics({
      ...current,
      thumbTop: (scrollTop / current.maxScrollTop) * maxThumbTop
    })
  }, [publishMetrics, readScrollMetrics])

  useEffect(() => {
    const element = scrollController?.getElement?.() ?? scrollRef?.current ?? null
    if (!element && !scrollController) {
      // A kept-alive terminal can dispose its xterm instance before this
      // component unmounts. Do not leave the previous controller's thumb
      // rendered against a now-detached terminal.
      publishMetrics(EMPTY_METRICS)
      return
    }

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
    const handleNativeScroll = () => {
      reveal()
      scheduleThumbUpdate()
    }
    const handleControllerChange = () => {
      reveal()
      // An xterm write can change the buffer length without changing the
      // viewport position. Recompute the full model so the scrollbar can
      // appear as soon as scrollback is created.
      scheduleMetricsUpdate()
    }

    scheduleMetricsUpdate()
    if (scrollController) {
      const unsubscribe = scrollController.subscribe(handleControllerChange)
      if (element) {
        resizeObserver.observe(element)
        if (element.firstElementChild) resizeObserver.observe(element.firstElementChild)
      }

      return () => {
        cancelAnimationFrame(frame)
        unsubscribe()
        resizeObserver.disconnect()
      }
    }

    element?.addEventListener('scroll', handleNativeScroll, { passive: true })
    if (element) {
      resizeObserver.observe(element)
      if (element.firstElementChild) resizeObserver.observe(element.firstElementChild)
    }

    return () => {
      cancelAnimationFrame(frame)
      element?.removeEventListener('scroll', handleNativeScroll)
      resizeObserver.disconnect()
    }
  }, [reveal, scrollController, scrollRef, updateMetrics, updateThumbPosition])

  const setScrollFromThumbTop = (thumbTop: number) => {
    const currentScroll = readScrollMetrics()
    const current = metricsRef.current
    if (!currentScroll || current.maxScrollTop === 0) return

    const maxThumbTop = current.trackHeight - current.thumbHeight
    if (maxThumbTop <= 0) return
    const nextScrollTop = (Math.max(0, Math.min(maxThumbTop, thumbTop)) / maxThumbTop) * current.maxScrollTop
    if (scrollController) {
      scrollController.scrollTo(nextScrollTop)
    } else if (scrollRef?.current) {
      scrollRef.current.scrollTop = nextScrollTop
    }
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = readScrollMetrics()
    if (!current) return

    const page = Math.max(32, current.clientHeight - 32)
    const offsets: Record<string, number> = {
      ArrowDown: 32,
      ArrowUp: -32,
      PageDown: page,
      PageUp: -page
    }
    if (event.key === 'Home') {
      if (scrollController) {
        scrollController.scrollTo(0)
      } else if (scrollRef?.current) {
        scrollRef.current.scrollTop = 0
      }
    } else if (event.key === 'End') {
      if (scrollController) {
        scrollController.scrollTo(current.scrollHeight)
      } else if (scrollRef?.current) {
        scrollRef.current.scrollTop = scrollRef.current.scrollHeight
      }
    } else if (event.key in offsets) {
      if (scrollController) {
        scrollController.scrollBy(offsets[event.key])
      } else {
        scrollRef?.current?.scrollBy({ top: offsets[event.key] })
      }
    } else {
      return
    }
    event.preventDefault()
  }

  const handleWheel = (event: ReactWheelEvent<HTMLDivElement>) => {
    const current = readScrollMetrics()
    if (!current || current.scrollHeight <= current.clientHeight || event.deltaY === 0) {
      return
    }

    const delta =
      event.deltaMode === WheelEvent.DOM_DELTA_LINE
        ? event.deltaY * 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
          ? event.deltaY * current.clientHeight
          : event.deltaY
    if (scrollController) {
      scrollController.scrollBy(delta)
    } else {
      scrollRef?.current?.scrollBy({ top: delta })
    }
    reveal()
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
      aria-valuenow={Math.round(readScrollMetrics()?.scrollTop ?? 0)}
      className={`vertical-scrollbar${isVisible ? ' is-visible' : ''}`}
      onBlur={() => {
        if (!dragRef.current) reveal()
      }}
      onFocus={reveal}
      onKeyDown={handleKeyDown}
      onWheel={handleWheel}
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
