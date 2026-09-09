import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { PathBookmark, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import { Button } from '../../components/common/button/button'
import { Dialog } from '../../components/common/dialog/dialog'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { toPathBookmark } from './path-bookmark-provider'
import './path-bookmarks-button.css'

export function PathBookmarksButton({
  scope,
  path,
  disabled = false,
  onOpen
}: {
  scope: string
  path: string
  disabled?: boolean
  onOpen(item: RemoteFileItem): void
}) {
  const [open, setOpen] = useState(false)
  const [items, setItems] = useState<PathBookmark[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [removing, setRemoving] = useState<PathBookmark | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const dialogRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!open || !window.fileterm) return
    let cancelled = false
    setBusy(true)
    setError(null)
    void window.fileterm
      .getPathBookmarks(scope)
      .then((next) => {
        if (!cancelled) setItems(next)
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(String(cause))
      })
      .finally(() => {
        if (!cancelled) setBusy(false)
      })
    return () => {
      cancelled = true
    }
  }, [open, scope])
  useEffect(() => {
    if (!open) return
    const trigger = triggerRef.current
    return () => trigger?.focus()
  }, [open])
  useEffect(() => {
    if (open && !removing) dialogRef.current?.focus()
  }, [open, removing])
  const update = async (action: 'add' | 'remove' | 'up' | 'down', item: PathBookmark) => {
    if (busy || !window.fileterm) return
    setBusy(true)
    setError(null)
    try {
      setItems(await window.fileterm.updatePathBookmark(scope, action, item))
      setRemoving(null)
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
      if (action !== 'remove') dialogRef.current?.focus()
    }
  }
  const current = toPathBookmark(scope, {
    path,
    name: path.split(/[/\\]/).filter(Boolean).at(-1) || path,
    type: 'folder'
  })
  const isCurrentBookmarked = items.some((item) => item.path === path)
  const canBookmarkCurrent = Boolean(
    !disabled && !busy && path && !path.startsWith('fileterm:') && !isCurrentBookmarked
  )

  return (
    <span className="path-bookmarks-control" onKeyDown={(event) => event.stopPropagation()}>
      <Button
        ref={triggerRef}
        className="path-bookmarks-trigger"
        variant="ghost"
        size="compact"
        disabled={disabled || !window.fileterm?.getPathBookmarks}
        title={t.pathBookmarks}
        aria-label={t.pathBookmarks}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        icon={<AppIcon name="star" size={13} />}
      />
      {open &&
        createPortal(
          <Dialog
            ref={dialogRef}
            tabIndex={-1}
            isOpen={open}
            title={t.pathBookmarks}
            aria-label={t.pathBookmarks}
            size="sm"
            className="path-bookmarks-dialog"
            onClose={() => {
              if (!busy && !removing) setOpen(false)
            }}
            onKeyDown={(event) => {
              if (event.key !== 'Tab') return
              const targets = Array.from(
                dialogRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') || []
              )
              const first = targets[0],
                last = targets.at(-1)
              if (
                event.shiftKey &&
                (document.activeElement === first || document.activeElement === dialogRef.current)
              ) {
                event.preventDefault()
                last?.focus()
              } else if (!event.shiftKey && document.activeElement === last) {
                event.preventDefault()
                first?.focus()
              }
            }}
          >
            <div className="path-bookmarks">
              <div className="path-bookmarks-header-card">
                <div className="path-bookmarks-current-info">
                  <span className="path-bookmarks-current-label">{t.currentDirectory}</span>
                  <span className="path-bookmarks-current-path" title={path || '-'}>
                    {path || '-'}
                  </span>
                </div>
                <Button
                  className="path-bookmarks-add-btn"
                  variant={isCurrentBookmarked ? 'secondary' : 'primary'}
                  size="compact"
                  disabled={!canBookmarkCurrent}
                  onClick={() => void update('add', current)}
                  icon={<AppIcon name={isCurrentBookmarked ? 'check' : 'plus'} size={13} />}
                >
                  {isCurrentBookmarked ? t.bookmarkAlreadyAdded : t.bookmarkCurrent}
                </Button>
              </div>
              {error && (
                <p role="alert" className="path-bookmarks-error">
                  {error}
                </p>
              )}
              <div className="path-bookmarks-region">
                <div className="path-bookmarks-list" ref={scrollRef} aria-busy={busy}>
                  {!busy && items.length === 0 && (
                    <div className="path-bookmarks-empty">
                      <div className="path-bookmarks-empty-icon-box">
                        <AppIcon name="star" size={22} />
                      </div>
                      <p className="path-bookmarks-empty-title">{t.bookmarkEmptyTitle}</p>
                      <p className="path-bookmarks-empty-hint">{t.bookmarkEmptyHint}</p>
                    </div>
                  )}
                  {items.map((item, index) => (
                    <div className="path-bookmarks-row" key={item.id}>
                      <Button
                        className="path-bookmarks-open"
                        variant="ghost"
                        size="regular"
                        disabled={disabled || busy}
                        title={item.path}
                        onClick={() => {
                          onOpen({ path: item.path, name: item.name, type: item.type, size: '', modified: '' })
                          setOpen(false)
                        }}
                        icon={
                          <AppIcon
                            name={item.type === 'folder' ? 'folder' : 'file'}
                            size={16}
                            className="path-bookmarks-row-icon"
                          />
                        }
                      >
                        <span className="path-bookmarks-open-content">
                          <span className="path-bookmarks-open-name">{item.name}</span>
                          <small className="path-bookmarks-open-path">{item.path}</small>
                        </span>
                      </Button>
                      <div className="path-bookmarks-row-actions">
                        <Button
                          className="path-bookmarks-action-btn"
                          variant="ghost"
                          size="compact"
                          disabled={busy || index === 0}
                          aria-label={`${t.bookmarkUp}: ${item.name}`}
                          title={t.bookmarkUp}
                          onClick={() => void update('up', item)}
                          icon={<AppIcon name="arrow-up" size={13} />}
                        />
                        <Button
                          className="path-bookmarks-action-btn"
                          variant="ghost"
                          size="compact"
                          disabled={busy || index === items.length - 1}
                          aria-label={`${t.bookmarkDown}: ${item.name}`}
                          title={t.bookmarkDown}
                          onClick={() => void update('down', item)}
                          icon={<AppIcon name="arrow-down" size={13} />}
                        />
                        <Button
                          className="path-bookmarks-action-btn path-bookmarks-delete-btn"
                          variant="ghost"
                          size="compact"
                          disabled={busy}
                          aria-label={`${t.bookmarkRemove}: ${item.name}`}
                          title={t.bookmarkRemove}
                          onClick={() => setRemoving(item)}
                          icon={<AppIcon name="trash" size={13} />}
                        />
                      </div>
                    </div>
                  ))}
                </div>
                <VerticalScrollbar scrollRef={scrollRef} ariaLabel={t.pathBookmarks} />
              </div>
            </div>
          </Dialog>,
          document.body
        )}
      {removing && (
        <ConfirmActionDialog
          title={t.bookmarkRemove}
          description={removing.path}
          confirmLabel={t.bookmarkRemove}
          isSubmitting={busy}
          errorMessage={error}
          onClose={() => setRemoving(null)}
          onConfirm={() => void update('remove', removing)}
        />
      )}
    </span>
  )
}
