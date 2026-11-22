import type { Dispatch, RefObject, SetStateAction } from 'react'
import { AppIcon } from '../features/common/app-icon'
import { CloseButton } from '../features/common/close-button'
import { t } from '../i18n'

type TerminalFindBarProps = {
  findInputRef: RefObject<HTMLInputElement | null>
  findQuery: string
  setFindQuery: Dispatch<SetStateAction<string>>
  setActiveFindIndex: Dispatch<SetStateAction<number>>
  findMiss: boolean
  findMatchCount: number
  activeFindIndex: number
  findCaseSensitive: boolean
  setFindCaseSensitive: Dispatch<SetStateAction<boolean>>
  findRegex: boolean
  setFindRegex: Dispatch<SetStateAction<boolean>>
  closeFind(): void
  searchTerminal(query: string, direction?: 1 | -1): boolean
}

export function TerminalFindBar({
  findInputRef,
  findQuery,
  setFindQuery,
  setActiveFindIndex,
  findMiss,
  findMatchCount,
  activeFindIndex,
  findCaseSensitive,
  setFindCaseSensitive,
  findRegex,
  setFindRegex,
  closeFind,
  searchTerminal
}: TerminalFindBarProps) {
  return (
    <div className="terminal-find" onClick={(event) => event.stopPropagation()}>
      <div className="terminal-find-query">
        <AppIcon name="search" size={14} />
        <input
          ref={findInputRef}
          aria-label={t.find}
          type="text"
          value={findQuery}
          onChange={(event) => {
            setFindQuery(event.target.value)
            setActiveFindIndex(-1)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              searchTerminal(findQuery, event.shiftKey ? -1 : 1)
            }
            if (event.key === 'Escape') {
              event.preventDefault()
              closeFind()
            }
          }}
        />
      </div>
      <div className="terminal-find-count" aria-live="polite">
        {findQuery && findMatchCount > 0 ? `${Math.max(activeFindIndex + 1, 1)}/${findMatchCount}` : null}
      </div>
      <div className="terminal-find-actions" role="group" aria-label={t.find}>
        <button
          type="button"
          className={findCaseSensitive ? 'is-active' : undefined}
          aria-pressed={findCaseSensitive}
          title={t.findCaseSensitive}
          onClick={() => setFindCaseSensitive((value) => !value)}
        >
          Aa
        </button>
        <button
          type="button"
          className={findRegex ? 'is-active' : undefined}
          aria-pressed={findRegex}
          title={t.findRegex}
          onClick={() => setFindRegex((value) => !value)}
        >
          .*
        </button>
        <button type="button" title={t.findPrevious} onClick={() => searchTerminal(findQuery, -1)}>
          <AppIcon name="arrow-up" size={13} />
        </button>
        <button type="button" title={t.findNext} onClick={() => searchTerminal(findQuery, 1)}>
          <AppIcon name="arrow-down" size={13} />
        </button>
        <button
          aria-label={t.find}
          className="terminal-find-submit"
          type="button"
          title={t.find}
          onClick={() => searchTerminal(findQuery, 1)}
        >
          <AppIcon name="search" size={14} />
        </button>
      </div>
      <CloseButton onClick={closeFind} size="compact" />
      {findMiss ? <span className="terminal-find-status">{t.findNotFound}</span> : null}
    </div>
  )
}
