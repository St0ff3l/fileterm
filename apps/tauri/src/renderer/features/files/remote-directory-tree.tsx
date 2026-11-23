import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent } from 'react'
import type { RemoteFileItem } from '@fileterm/core'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { remoteDirectoryAncestors, remoteDirectoryRows, type RemoteDirectoryRow } from './remote-directory-tree-model'
import { useRemoteDirectoryTree } from './use-remote-directory-tree'
import './remote-directory-tree.css'

export function RemoteDirectoryTree({
  tabId,
  path: sourcePath,
  files,
  refresh,
  onOpenPath,
  onDropUpload
}: {
  tabId: string
  path: string
  files: RemoteFileItem[]
  refresh: number
  onOpenPath(path: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
}) {
  const path = remoteDirectoryAncestors(sourcePath).at(-1)!
  const tree = useRemoteDirectoryTree(tabId, path, files, refresh)
  const rows = remoteDirectoryRows(tree.cache, tree.expanded, path)
  const scrollRef = useRef<HTMLDivElement>(null)
  const buttons = useRef(new Map<string, HTMLDivElement>())
  const [focused, setFocused] = useState(path)
  const pendingFocus = useRef<string | null>(null)
  const focusRow = (target: string) => {
    setFocused(target)
    buttons.current.get(target)?.focus()
    buttons.current.get(target)?.scrollIntoView({ block: 'nearest' })
  }

  useEffect(() => {
    setFocused(path)
    buttons.current.get(path)?.scrollIntoView({ block: 'nearest' })
  }, [path])

  useEffect(() => {
    if (pendingFocus.current && buttons.current.has(pendingFocus.current)) {
      focusRow(pendingFocus.current)
      pendingFocus.current = null
    }
  })

  const locate = () => {
    remoteDirectoryAncestors(path).forEach((ancestor) => tree.toggle(ancestor, true))
    pendingFocus.current = path
    focusRow(path)
  }

  const onKeyDown = (event: KeyboardEvent, row: RemoteDirectoryRow, index: number) => {
    let target: string | undefined
    switch (event.key) {
      case 'ArrowDown':
        target = rows[Math.min(index + 1, rows.length - 1)]?.path
        break
      case 'ArrowUp':
        target = rows[Math.max(index - 1, 0)]?.path
        break
      case 'Home':
        target = rows[0]?.path
        break
      case 'End':
        target = rows.at(-1)?.path
        break
      case 'ArrowRight':
        if (!tree.expanded.has(row.path)) tree.toggle(row.path, true)
        else if (rows[index + 1]?.parent === row.path) target = rows[index + 1]?.path
        break
      case 'ArrowLeft':
        if (tree.expanded.has(row.path)) tree.toggle(row.path, false)
        else target = row.parent ?? undefined
        break
      case 'Enter':
      case ' ':
        onOpenPath(row.path)
        break
      default:
        return
    }
    event.preventDefault()
    event.stopPropagation()
    if (target) focusRow(target)
  }

  return (
    <div
      className="remote-directory-pane"
      data-file-drop-pane="remote"
      data-file-drop-current-path={path}
      onDragOver={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'copy'
        dispatchAppEvent(APP_EVENT.tauriRemoteDragOver)
      }}
      onDrop={onDropUpload}
    >
      <div className="remote-directory-tree-header">
        <span>{t.remoteDirectoryTree}</span>
        <button
          type="button"
          className="remote-directory-locate"
          title={t.locateCurrentDirectory}
          aria-label={t.locateCurrentDirectory}
          onClick={locate}
        >
          <span aria-hidden="true" className="remote-directory-locate-icon" />
        </button>
      </div>
      <div className="remote-directory-scroll-region">
        <div role="tree" aria-label={t.remoteDirectoryTree} className="remote-directory-tree" ref={scrollRef}>
          {rows.map((row, index) => (
            <div key={row.path}>
              <div
                role="treeitem"
                aria-level={row.depth + 1}
                aria-selected={row.path === path}
                aria-expanded={tree.expanded.has(row.path)}
                aria-busy={tree.loading.has(row.path)}
                aria-label={row.name}
                tabIndex={row.path === focused || (!rows.some((item) => item.path === focused) && index === 0) ? 0 : -1}
                className={`remote-directory-row ${row.path === path ? 'is-current' : ''}`}
                data-file-drop-pane="remote"
                data-file-drop-folder-path={row.path}
                data-file-drop-folder-name={row.name}
                ref={(element) => {
                  if (element) buttons.current.set(row.path, element)
                  else buttons.current.delete(row.path)
                }}
                style={{ paddingInlineStart: 8 + row.depth * 16 }}
                onFocus={() => setFocused(row.path)}
                onKeyDown={(event) => onKeyDown(event, row, index)}
                onClick={() => {
                  setFocused(row.path)
                  onOpenPath(row.path)
                }}
              >
                <button
                  type="button"
                  tabIndex={-1}
                  className="remote-directory-expand"
                  aria-label={tree.expanded.has(row.path) ? t.directoryTreeCollapse : t.directoryTreeExpand}
                  onClick={(event) => {
                    event.stopPropagation()
                    tree.toggle(row.path)
                    focusRow(row.path)
                  }}
                >
                  <AppIcon name={tree.expanded.has(row.path) ? 'chevron-down' : 'chevron-right'} size={12} />
                </button>
                <AppIcon name="folder" size={14} />
                <span className="remote-directory-name" title={row.path}>
                  {row.name}
                </span>
                {tree.loading.has(row.path) ? <span className="remote-directory-loading" aria-hidden="true" /> : null}
              </div>
              {tree.expanded.has(row.path) && tree.errors[row.path] ? (
                <div className="remote-directory-error" role="status">
                  <span title={tree.errors[row.path]}>{tree.errors[row.path]}</span>
                  <button type="button" onClick={() => void tree.load(row.path, true)}>
                    {t.directoryTreeRetry}
                  </button>
                </div>
              ) : null}
            </div>
          ))}
        </div>
        <VerticalScrollbar ariaLabel={t.scrollRemoteFiles} scrollRef={scrollRef} />
      </div>
    </div>
  )
}
