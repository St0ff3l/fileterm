import { useCallback, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react'
import { createPortal } from 'react-dom'
import { AppIcon } from '../../../features/common/app-icon'
import { CloseButton } from '../../../features/common/close-button'
import { FeedbackText, type FeedbackTextTone } from '../../../features/common/feedback-text'
import { VerticalScrollbar } from '../../../features/common/vertical-scrollbar'
import { t } from '../../../i18n'
import './feedback-bubble.css'

type FeedbackBubbleNotice = {
  id: number
  duration?: number
  message: string
  onClose?: () => void
  title?: string | null
  tone: FeedbackTextTone
}

let notices: FeedbackBubbleNotice[] = []
let nextNoticeId = 0
const noticeListeners = new Set<() => void>()
const lastNoticeKeysBySource = new Map<string, string>()

function subscribeToNotices(listener: () => void) {
  noticeListeners.add(listener)
  return () => noticeListeners.delete(listener)
}

function publishNotices() {
  for (const listener of noticeListeners) listener()
}

function enqueueNotice(notice: Omit<FeedbackBubbleNotice, 'id'>) {
  notices = [{ ...notice, id: ++nextNoticeId }, ...notices]
  publishNotices()
}

function dismissNotice(id: number) {
  const remaining = notices.filter((notice) => notice.id !== id)
  if (remaining.length === notices.length) return
  notices = remaining
  publishNotices()
}

function FeedbackBubbleIcon({ tone }: { tone: FeedbackTextTone }) {
  if (tone === 'loading') {
    return <span aria-hidden="true" className="feedback-bubble__spinner button-spinner" />
  }
  if (tone === 'success') {
    return (
      <span aria-hidden="true" className="feedback-bubble__icon feedback-bubble__icon--success">
        <AppIcon name="check" size={16} strokeWidth={2.4} />
      </span>
    )
  }
  if (tone === 'error') {
    return (
      <span aria-hidden="true" className="feedback-bubble__icon feedback-bubble__icon--error">
        <AppIcon name="close" size={15} strokeWidth={2.4} />
      </span>
    )
  }
  if (tone === 'warning') {
    return (
      <span aria-hidden="true" className="feedback-bubble__icon feedback-bubble__icon--warning">
        <AppIcon name="info" size={16} strokeWidth={2.2} />
      </span>
    )
  }
  return (
    <span aria-hidden="true" className="feedback-bubble__icon feedback-bubble__icon--info">
      <AppIcon name="info" size={16} strokeWidth={2.2} />
    </span>
  )
}

function resolveDefaultTitle(tone: FeedbackTextTone): string | null {
  if (tone === 'success') return t.feedbackSuccess
  if (tone === 'error') return t.feedbackError
  if (tone === 'warning') return t.feedbackWarning
  if (tone === 'info') return t.feedbackInfo
  return null
}

export function FeedbackBubble({
  duration,
  eventId,
  message,
  onClose,
  sourceKey,
  title,
  tone = 'info'
}: {
  duration?: number
  eventId?: number | string
  message?: string | null
  onClose?: () => void
  sourceKey?: string
  title?: string | null
  tone?: FeedbackTextTone
}) {
  const lastNoticeKeyRef = useRef<string | null>(null)
  const onCloseRef = useRef(onClose)

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    if (!message) {
      lastNoticeKeyRef.current = null
      if (sourceKey) lastNoticeKeysBySource.delete(sourceKey)
      return
    }

    const key = JSON.stringify([eventId, message, tone, title, duration])
    if (lastNoticeKeyRef.current === key) return
    lastNoticeKeyRef.current = key
    if (sourceKey && lastNoticeKeysBySource.get(sourceKey) === key) return
    if (sourceKey) lastNoticeKeysBySource.set(sourceKey, key)
    enqueueNotice({ duration, message, onClose: onCloseRef.current, title, tone })
  }, [duration, eventId, message, sourceKey, title, tone])

  return null
}

function FeedbackBubbleItem({ notice }: { notice: FeedbackBubbleNotice }) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const exitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const isExitingRef = useRef(false)
  const [isExiting, setIsExiting] = useState(false)
  const [isHovered, setIsHovered] = useState(false)
  const autoDismissDelay = notice.duration ?? (notice.tone === 'loading' ? 0 : 4000)

  const triggerDismiss = useCallback(() => {
    if (isExitingRef.current) return
    isExitingRef.current = true
    setIsExiting(true)
    exitTimerRef.current = setTimeout(() => {
      exitTimerRef.current = null
      dismissNotice(notice.id)
      notice.onClose?.()
    }, 180)
  }, [notice])

  useEffect(() => {
    if (autoDismissDelay <= 0 || isHovered || isExiting) return
    const timer = setTimeout(triggerDismiss, autoDismissDelay)
    return () => clearTimeout(timer)
  }, [autoDismissDelay, isExiting, isHovered, triggerDismiss])

  useEffect(
    () => () => {
      if (exitTimerRef.current) clearTimeout(exitTimerRef.current)
    },
    []
  )

  const displayTitle = notice.title !== undefined ? notice.title : resolveDefaultTitle(notice.tone)
  const resolvedClassName = ['feedback-bubble', `feedback-bubble--${notice.tone}`, isExiting ? 'is-exiting' : '']
    .filter(Boolean)
    .join(' ')

  return (
    <div
      className={resolvedClassName}
      data-feedback-notice-id={notice.id}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      role={notice.tone === 'error' ? 'alert' : 'status'}
    >
      <FeedbackBubbleIcon tone={notice.tone} />
      <div ref={scrollRef} className="feedback-bubble__body">
        {displayTitle ? <div className="feedback-bubble__title">{displayTitle}</div> : null}
        <div className="feedback-bubble__content">
          <FeedbackText message={notice.message} tone={notice.tone} />
        </div>
      </div>
      <VerticalScrollbar scrollRef={scrollRef} />
      <CloseButton
        aria-label={t.closeTab}
        className="feedback-bubble__close-button"
        onClick={triggerDismiss}
        size="compact"
      />
    </div>
  )
}

export function FeedbackBubbleViewport() {
  const currentNotices = useSyncExternalStore(
    subscribeToNotices,
    () => notices,
    () => notices
  )
  const viewportRef = useRef<HTMLDivElement>(null)
  const previousTopsRef = useRef(new Map<number, number>())
  const reorderAnimationsRef = useRef(new Map<number, Animation>())

  useLayoutEffect(() => {
    const viewport = viewportRef.current
    if (!viewport) return

    const motion = getComputedStyle(viewport)
    const viewportTop = viewport.getBoundingClientRect().top
    const nextTops = new Map<number, number>()
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    for (const item of viewport.querySelectorAll<HTMLElement>('[data-feedback-notice-id]')) {
      const id = Number(item.dataset.feedbackNoticeId)
      const top = viewportTop + item.offsetTop
      let previousTop = previousTopsRef.current.get(id)
      const activeAnimation = reorderAnimationsRef.current.get(id)
      if (activeAnimation) {
        const offset = Number.parseFloat(getComputedStyle(item).translate.split(' ')[1]) || 0
        if (previousTop !== undefined) previousTop += offset
        activeAnimation.cancel()
        reorderAnimationsRef.current.delete(id)
      }

      if (!reduceMotion && previousTop !== undefined && previousTop !== top) {
        const animation = item.animate([{ translate: `0px ${previousTop - top}px` }, { translate: '0px 0px' }], {
          composite: 'add',
          duration: Number.parseFloat(motion.getPropertyValue('--duration-move')) || 180,
          easing: motion.getPropertyValue('--ease-in-out').trim() || 'ease-in-out'
        })
        reorderAnimationsRef.current.set(id, animation)
      }

      nextTops.set(id, top)
    }

    for (const [id, animation] of reorderAnimationsRef.current) {
      if (!nextTops.has(id)) {
        animation.cancel()
        reorderAnimationsRef.current.delete(id)
      }
    }
    previousTopsRef.current = nextTops
  }, [currentNotices])

  useLayoutEffect(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)')
    const stop = () => {
      for (const animation of reorderAnimationsRef.current.values()) animation.cancel()
      reorderAnimationsRef.current.clear()
    }
    const onPreferenceChange = () => {
      if (preference.matches) stop()
    }
    preference.addEventListener('change', onPreferenceChange)
    return () => {
      preference.removeEventListener('change', onPreferenceChange)
      stop()
    }
  }, [])

  if (currentNotices.length === 0 || typeof document === 'undefined') return null

  return createPortal(
    <div ref={viewportRef} className="feedback-bubble-viewport">
      {currentNotices.map((notice) => (
        <FeedbackBubbleItem key={notice.id} notice={notice} />
      ))}
    </div>,
    document.body
  )
}
