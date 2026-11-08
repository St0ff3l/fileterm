import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { AppIcon } from '../../../features/common/app-icon'
import { CloseButton } from '../../../features/common/close-button'
import { FeedbackText, type FeedbackTextTone } from '../../../features/common/feedback-text'
import { VerticalScrollbar } from '../../../features/common/vertical-scrollbar'
import { t } from '../../../i18n'
import './feedback-bubble.css'

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
  message,
  onClose,
  title,
  tone = 'info'
}: {
  duration?: number
  message?: string | null
  onClose?: () => void
  title?: string | null
  tone?: FeedbackTextTone
}) {
  const [dismissedMessage, setDismissedMessage] = useState<string | null>(null)
  const [isExiting, setIsExiting] = useState(false)
  const scrollRef = useRef<HTMLDivElement>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const exitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const isVisible = Boolean(message && dismissedMessage !== message)

  const clearTimers = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }
    if (exitTimerRef.current) {
      clearTimeout(exitTimerRef.current)
      exitTimerRef.current = null
    }
  }, [])

  const triggerDismiss = useCallback(() => {
    if (!message) return
    setIsExiting(true)
    clearTimers()
    exitTimerRef.current = setTimeout(() => {
      setDismissedMessage(message)
      setIsExiting(false)
      onClose?.()
    }, 140)
  }, [clearTimers, message, onClose])

  const autoDismissDelay = duration ?? (tone === 'success' || tone === 'info' ? 4000 : 0)

  const startAutoDismiss = useCallback(() => {
    if (autoDismissDelay > 0 && isVisible && !isExiting) {
      clearTimers()
      timerRef.current = setTimeout(() => {
        triggerDismiss()
      }, autoDismissDelay)
    }
  }, [autoDismissDelay, clearTimers, isExiting, isVisible, triggerDismiss])

  useEffect(() => {
    if (isVisible) {
      startAutoDismiss()
    }
    return clearTimers
  }, [isVisible, startAutoDismiss, clearTimers])

  if (!isVisible) return null

  const displayTitle = title !== undefined ? title : resolveDefaultTitle(tone)

  const resolvedClassName = ['feedback-bubble', `feedback-bubble--${tone}`, isExiting ? 'is-exiting' : '']
    .filter(Boolean)
    .join(' ')

  return createPortal(
    <div
      className={resolvedClassName}
      onMouseEnter={clearTimers}
      onMouseLeave={startAutoDismiss}
      role={tone === 'error' ? 'alert' : 'status'}
    >
      <FeedbackBubbleIcon tone={tone} />
      <div ref={scrollRef} className="feedback-bubble__body">
        {displayTitle ? <div className="feedback-bubble__title">{displayTitle}</div> : null}
        <div className="feedback-bubble__content">
          <FeedbackText message={message} tone={tone} />
        </div>
      </div>
      <VerticalScrollbar scrollRef={scrollRef} />
      <CloseButton
        aria-label={t.closeTab}
        className="feedback-bubble__close-button"
        onClick={triggerDismiss}
        size="compact"
      />
    </div>,
    document.body
  )
}
