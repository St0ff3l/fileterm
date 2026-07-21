import { useEffect, useRef, type KeyboardEvent } from 'react'

const COMMAND_EDITOR_MIN_LINE_COUNT = 14

export function CommandCodeEditor({
  value,
  onChange,
  onKeyDown,
  placeholder,
  autoFocus = false,
  ariaLabel,
  className
}: {
  value: string
  onChange(value: string): void
  onKeyDown?(event: KeyboardEvent<HTMLTextAreaElement>): void
  placeholder?: string
  autoFocus?: boolean
  ariaLabel?: string
  className?: string
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const lineNumRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (autoFocus) {
      textareaRef.current?.focus()
    }
  }, [autoFocus])

  const handleTextareaScroll = () => {
    if (lineNumRef.current && textareaRef.current) {
      lineNumRef.current.scrollTop = textareaRef.current.scrollTop
    }
  }

  const lineCount = Math.max(value.split('\n').length, COMMAND_EDITOR_MIN_LINE_COUNT)

  return (
    <div className="command-code-area">
      <div className="command-line-numbers" ref={lineNumRef} aria-hidden="true">
        {Array.from({ length: lineCount }, (_, index) => (
          <div key={index} className="command-line-number">
            {index + 1}
          </div>
        ))}
      </div>
      <textarea
        ref={textareaRef}
        aria-label={ariaLabel}
        placeholder={placeholder}
        rows={12}
        spellCheck={false}
        className={className}
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
        onKeyDown={onKeyDown}
        onScroll={handleTextareaScroll}
      />
    </div>
  )
}
