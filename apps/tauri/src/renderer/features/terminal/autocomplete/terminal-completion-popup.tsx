import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { VerticalScrollbar } from '../../common/vertical-scrollbar'
import { t } from '../../../i18n'
import type { CompletionPopup } from './use-terminal-completion'
import './terminal-completion-popup.css'

export function TerminalCompletionPopup({
  popup,
  onAccept,
  onSelect
}: {
  popup: CompletionPopup
  onAccept(index: number): void
  onSelect(index: number): void
}) {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    scrollRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [popup.selected])
  return createPortal(
    <div
      className={`terminal-completion${popup.above ? ' terminal-completion--above' : ''}`}
      style={{ left: popup.left, top: popup.top, width: popup.width, maxHeight: popup.height }}
      onMouseDown={(event) => event.preventDefault()}
    >
      <div className="terminal-completion-scroll">
        <div
          className="terminal-completion-list"
          role="listbox"
          aria-label={t.terminalCommandCompletion}
          ref={scrollRef}
        >
          {popup.items.map((item, index) => (
            <div
              className="terminal-completion-option"
              role="option"
              aria-selected={index === popup.selected}
              key={item.value}
              onClick={() => onAccept(index)}
              onPointerMove={() => onSelect(index)}
            >
              <span className="terminal-completion-name">{item.label ?? item.value}</span>
              <span className="terminal-completion-description">{item.description}</span>
            </div>
          ))}
        </div>
        <VerticalScrollbar scrollRef={scrollRef} />
      </div>
      <div className="terminal-completion-help">{t.terminalCompletionHelp}</div>
      <span className="terminal-completion-announcement" role="status" aria-live="polite">
        {popup.items[popup.selected]?.value}
      </span>
    </div>,
    document.body
  )
}
