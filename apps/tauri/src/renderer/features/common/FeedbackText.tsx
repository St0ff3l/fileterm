export type FeedbackTextTone = 'info' | 'success' | 'warning' | 'error' | 'loading'

export function FeedbackText({
  className,
  message,
  tone = 'info'
}: {
  className?: string
  message?: string | null
  tone?: FeedbackTextTone
}) {
  if (!message) return null

  const resolvedClassName = ['feedback-text', `feedback-text--${tone}`, className].filter(Boolean).join(' ')
  const isLoading = tone === 'loading'

  return (
    <span
      aria-busy={isLoading || undefined}
      aria-live="polite"
      className={resolvedClassName}
      role={tone === 'error' ? 'alert' : 'status'}
    >
      {isLoading ? <span aria-hidden="true" className="feedback-text__spinner button-spinner" /> : null}
      <span className="feedback-text__message">{message}</span>
    </span>
  )
}
