import type { CommandFolder, CommandTemplate } from '@fileterm/core'
import { useMemo, useRef, type ReactNode } from 'react'
import { t } from '../../i18n'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { groupCommands } from './command-utils'
import './command-template-library.css'

export function CommandTemplateLibrary({
  folders,
  templates,
  visibleTemplates,
  folderId,
  selectedId,
  onFolderChange,
  onSelect,
  leadingTab,
  children,
  disabled = false
}: {
  folders: CommandFolder[]
  templates: CommandTemplate[]
  visibleTemplates: CommandTemplate[]
  folderId: string
  selectedId?: string
  onFolderChange(id: string): void
  onSelect(id: string): void
  leadingTab?: ReactNode
  children?: ReactNode
  disabled?: boolean
}) {
  const ref = useRef<HTMLDivElement>(null)
  const grouped = useMemo(() => groupCommands(folders, templates), [folders, templates])
  const ungroupedCount = templates.filter((template) => !template.parentId).length
  const tabs = [
    { id: 'all', label: t.all, count: templates.length },
    ...grouped.map(({ folder, templates }) => ({ id: folder.id, label: folder.name, count: templates.length })),
    ...(ungroupedCount ? [{ id: 'ungrouped', label: t.commandUncategorized, count: ungroupedCount }] : [])
  ]
  return (
    <section className="command-pane command-pane-list" aria-label={t.commandTemplate}>
      <div className="command-folder-bar">
        <div className="command-folder-tabs-scroll" onWheel={handleHorizontalWheelScroll}>
          <div className="command-folder-tabs">
            {leadingTab}
            {tabs.map(({ id, label, count }) => (
              <button
                aria-pressed={folderId === id}
                className={folderId === id ? 'active' : ''}
                disabled={disabled}
                key={id}
                onClick={() => onFolderChange(id)}
                type="button"
              >
                <span>{label}</span>
                <small>{count}</small>
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className="command-template-list-region">
        <div className="command-template-list scrollbar-scroll" ref={ref}>
          {children ?? (
            <>
              <table className="fs-file-table compact command-table">
                <colgroup>
                  <col style={{ width: '100%' }} />
                </colgroup>
                <thead>
                  <tr>
                    <th scope="col">{t.name}</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleTemplates.map((template) => (
                    <tr key={template.id} className={selectedId === template.id ? 'is-selected' : ''}>
                      <td>
                        <button
                          aria-pressed={selectedId === template.id}
                          className="col-name-wrapper command-template-select"
                          disabled={disabled}
                          onClick={() => onSelect(template.id)}
                          type="button"
                        >
                          <strong>{template.name}</strong>
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {!visibleTemplates.length && (
                <div className="command-empty-state">
                  <span>{t.commandEmpty}</span>
                </div>
              )}
            </>
          )}
        </div>
        <VerticalScrollbar ariaLabel={t.scrollCommandList} scrollRef={ref} topInset={24} />
      </div>
    </section>
  )
}
