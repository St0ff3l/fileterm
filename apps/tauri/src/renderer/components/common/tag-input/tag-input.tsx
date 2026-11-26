import { useId, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { AppIcon } from '../../../features/common/app-icon'
import { t } from '../../../i18n'
import './tag-input.css'

export interface TagInputProps {
  id?: string
  ariaLabel?: string
  value: string[]
  suggestions?: string[]
  placeholder?: string
  disabled?: boolean
  className?: string
  onChange(tags: string[]): void
}

function normalizeTagList(tags: string[]): string[] {
  const seen = new Set<string>()
  const next: string[] = []
  for (const raw of tags) {
    const trimmed = raw.trim()
    if (!trimmed || seen.has(trimmed)) continue
    seen.add(trimmed)
    next.push(trimmed)
  }
  return next
}

export function TagInput({
  id,
  ariaLabel,
  value,
  suggestions = [],
  placeholder = t.connectionTagsPlaceholder,
  disabled = false,
  className = '',
  onChange
}: TagInputProps) {
  const generatedId = useId()
  const inputId = id ?? generatedId
  const listboxId = `${inputId}-suggestions`
  const inputRef = useRef<HTMLInputElement>(null)
  const [draft, setDraft] = useState('')
  const [focused, setFocused] = useState(false)
  const [activeIndex, setActiveIndex] = useState(-1)

  const tags = useMemo(() => normalizeTagList(value), [value])
  const filteredSuggestions = useMemo(() => {
    const query = draft.trim().toLocaleLowerCase()
    return suggestions
      .map((item) => item.trim())
      .filter((item) => item && !tags.includes(item) && (!query || item.toLocaleLowerCase().includes(query)))
      .slice(0, 8)
  }, [draft, suggestions, tags])

  const commitTags = (raw: string) => {
    const pieces = raw
      .split(/[\r\n,，]+/)
      .map((item) => item.trim())
      .filter(Boolean)
    if (!pieces.length) {
      setDraft('')
      return
    }
    const next = normalizeTagList([...tags, ...pieces])
    onChange(next)
    setDraft('')
    setActiveIndex(-1)
  }

  const removeTag = (target: string) => {
    onChange(tags.filter((tag) => tag !== target))
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (disabled) return
    if (event.key === 'ArrowDown' && filteredSuggestions.length > 0) {
      event.preventDefault()
      setActiveIndex((current) => (current + 1) % filteredSuggestions.length)
      return
    }
    if (event.key === 'ArrowUp' && filteredSuggestions.length > 0) {
      event.preventDefault()
      setActiveIndex((current) => (current <= 0 ? filteredSuggestions.length - 1 : current - 1))
      return
    }
    if (event.key === 'Escape') {
      setFocused(false)
      setActiveIndex(-1)
      return
    }
    if (event.key === 'Enter' || event.key === ',' || event.key === '，') {
      if (event.key === 'Enter' && activeIndex >= 0 && filteredSuggestions[activeIndex]) {
        event.preventDefault()
        commitTags(filteredSuggestions[activeIndex])
        return
      }
      if (draft.trim()) {
        event.preventDefault()
        commitTags(draft)
      }
      return
    }
    if (event.key === 'Backspace' && !draft && tags.length > 0) {
      event.preventDefault()
      onChange(tags.slice(0, -1))
    }
  }

  const showSuggestions = focused && !disabled && filteredSuggestions.length > 0
  const shellClasses = ['ft-tag-input', disabled ? 'ft-tag-input--disabled' : '', className].filter(Boolean).join(' ')

  return (
    <div className={shellClasses}>
      <div
        className="ft-tag-input__shell"
        onMouseDown={(event) => {
          if (disabled) return
          if ((event.target as HTMLElement).closest('button')) return
          inputRef.current?.focus()
        }}
      >
        {tags.map((tag) => (
          <span className="ft-tag-input__chip" key={tag}>
            <span className="ft-tag-input__chip-label" title={tag}>
              {tag}
            </span>
            {!disabled ? (
              <button
                aria-label={`${t.connectionTagRemove}: ${tag}`}
                className="ft-tag-input__chip-remove"
                type="button"
                onClick={() => removeTag(tag)}
              >
                <AppIcon name="close" size={11} />
              </button>
            ) : null}
          </span>
        ))}
        <input
          ref={inputRef}
          id={inputId}
          aria-autocomplete="list"
          aria-controls={showSuggestions ? listboxId : undefined}
          aria-expanded={showSuggestions}
          aria-label={ariaLabel}
          className="ft-tag-input__field"
          disabled={disabled}
          placeholder={tags.length === 0 ? placeholder : ''}
          role="combobox"
          type="text"
          value={draft}
          onBlur={() => {
            if (draft.trim()) commitTags(draft)
            setFocused(false)
            setActiveIndex(-1)
          }}
          onChange={(event) => {
            const nextValue = event.target.value
            if (/[,，\r\n]/.test(nextValue)) {
              commitTags(nextValue)
              return
            }
            setDraft(nextValue)
            setActiveIndex(-1)
          }}
          onFocus={() => setFocused(true)}
          onKeyDown={handleKeyDown}
        />
      </div>
      {showSuggestions ? (
        <div className="ft-tag-input__menu" id={listboxId} role="listbox">
          {filteredSuggestions.map((item, index) => (
            <button
              aria-selected={index === activeIndex}
              className={`ft-tag-input__option${index === activeIndex ? ' is-active' : ''}`}
              key={item}
              role="option"
              type="button"
              onMouseDown={(event) => {
                event.preventDefault()
                commitTags(item)
                inputRef.current?.focus()
              }}
            >
              {item}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
