import { useEffect, useMemo, useState, useRef } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandSendPreferences,
  CommandTemplate,
  WorkspaceTab
} from '@termdock/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'
import { SessionSendTargetPicker } from '../common/SessionSendTargetPicker'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { extractCommandParams, groupCommands, sortByOrder } from './command-utils'

function getCommandSnippet(command: string) {
  return command
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .join(' ')
}

function getCommandSummary(template: CommandTemplate) {
  return template.description?.trim() || getCommandSnippet(template.command) || t.commandNoDescription
}

export function CommandCenter({
  activeTab,
  commandFolders,
  commandTemplates,
  isBusy,
  sendTargets,
  onExecute,
  paneWidth,
  onPaneWidthChange,
}: {
  activeTab: WorkspaceTab | null
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  sendTargets: SessionSendTarget[]
  onExecute(commandId: string, args: string[], options: CommandExecutionOptions, scope: SendScope, selectedTabIds: string[]): void
  paneWidth: number
  onPaneWidthChange(width: number): void
}) {
  const grouped = useMemo(() => groupCommands(commandFolders, commandTemplates), [commandFolders, commandTemplates])
  const ungrouped = useMemo(
    () => sortByOrder(commandTemplates.filter((template) => !template.parentId)),
    [commandTemplates]
  )
  const [activeFolderId, setActiveFolderId] = useState<string>('all')
  const [selectedCommandId, setSelectedCommandId] = useState<string | null>(commandTemplates[0]?.id ?? null)
  const [paramValues, setParamValues] = useState<Record<number, string>>({})
  const [lastRenderedCommand, setLastRenderedCommand] = useState('')
  const [appendCarriageReturn, setAppendCarriageReturn] = useState(true)
  const [preferencesLoaded, setPreferencesLoaded] = useState(false)
  const [rememberSelection, setRememberSelection] = useState(false)
  const [sendScope, setSendScope] = useState<SendScope>('current')
  const [selectedTabIds, setSelectedTabIds] = useState<string[]>([])
  
  const splitRef = useRef<HTMLDivElement | null>(null)
  const isResizingCommandSplit = useRef(false)

  const visibleTemplates = useMemo(() => {
    if (activeFolderId === 'all') {
      return sortByOrder(commandTemplates)
    }
    if (activeFolderId === 'ungrouped') {
      return ungrouped
    }
    return sortByOrder(commandTemplates.filter((template) => template.parentId === activeFolderId))
  }, [activeFolderId, commandTemplates, ungrouped])

  const selectedTemplate = useMemo(
    () => visibleTemplates.find((template) => template.id === selectedCommandId)
      ?? commandTemplates.find((template) => template.id === selectedCommandId)
      ?? visibleTemplates[0]
      ?? null,
    [commandTemplates, selectedCommandId, visibleTemplates]
  )
  const paramIndexes = selectedTemplate ? extractCommandParams(selectedTemplate.command) : []
  const canRunCurrent = Boolean(activeTab && selectedTemplate && sendTargets.some((target) => target.tabId === activeTab.id))
  const canRunAny = Boolean(sendTargets.length && selectedTemplate)
  const canRunSelected = Boolean(
    selectedTemplate && selectedTabIds.some((tabId) => sendTargets.some((target) => target.tabId === tabId))
  )

  useEffect(() => {
    if (!selectedTemplate && commandTemplates[0]) {
      setSelectedCommandId(commandTemplates[0].id)
    }
  }, [commandTemplates, selectedTemplate])

  useEffect(() => {
    setParamValues({})
    setAppendCarriageReturn(selectedTemplate?.appendCarriageReturn ?? true)
    setLastRenderedCommand('')
  }, [selectedTemplate?.id])

  useEffect(() => {
    setSelectedTabIds((prev) => prev.filter((tabId) => sendTargets.some((target) => target.tabId === tabId)))
  }, [sendTargets])

  useEffect(() => {
    let canceled = false

    async function loadPreferences() {
      const desktopApi = window.termdock
      if (!desktopApi?.getCommandSendPreferences) {
        return
      }

      const storedPreferences = await desktopApi.getCommandSendPreferences()
      if (!canceled) {
        setRememberSelection(storedPreferences.rememberSelection)
        setSendScope(storedPreferences.rememberSelection ? storedPreferences.sendScope : 'current')
        setSelectedTabIds(storedPreferences.rememberSelection ? storedPreferences.selectedTabIds : [])
        setPreferencesLoaded(true)
      }
    }

    void loadPreferences()

    return () => {
      canceled = true
    }
  }, [])

  useEffect(() => {
    if (!preferencesLoaded || !window.termdock?.setCommandSendPreferences) {
      return
    }

    void window.termdock.setCommandSendPreferences({
      rememberSelection,
      sendScope: rememberSelection ? sendScope : 'current',
      selectedTabIds: rememberSelection ? selectedTabIds : []
    })
  }, [preferencesLoaded, rememberSelection, sendScope, selectedTabIds])

  const handleRun = () => {
    if (!selectedTemplate) {
      return
    }
    const args = paramIndexes.map((index) => paramValues[index] ?? '')
    const rendered = selectedTemplate.command.replace(/\[p#(\d+)\]/g, (_, rawIndex: string) => args[Number(rawIndex) - 1] ?? '')
    setLastRenderedCommand(rendered)
    onExecute(selectedTemplate.id, args, { appendCarriageReturn }, sendScope, selectedTabIds)
  }

  useEffect(() => {
    let dragFrame: number | null = null

    const handleMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingCommandSplit.current || !splitRef.current) return

      const rect = splitRef.current.getBoundingClientRect()
      const minListWidth = 180
      const minPreviewWidth = 320
      const maxListWidth = Math.max(minListWidth, rect.width - minPreviewWidth - 6)
      const nextWidth = Math.min(maxListWidth, Math.max(minListWidth, event.clientX - rect.left))
      
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }
      
      dragFrame = window.requestAnimationFrame(() => {
        onPaneWidthChange(nextWidth)
      })
    }

    const handleMouseUp = () => {
      if (!isResizingCommandSplit.current) return
      isResizingCommandSplit.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [onPaneWidthChange])

  return (
    <section className="command-center">
      <div 
        className="command-center-body" 
        ref={splitRef}
        style={{ '--list-pane-width': `${paneWidth}px` } as React.CSSProperties}
      >
        <section className="command-pane command-pane-list">
          <div className="command-folder-bar">
            <div
              className="command-folder-tabs"
              onWheel={handleHorizontalWheelScroll}
            >
              <button
                className={activeFolderId === 'all' ? 'active' : ''}
                type="button"
                onClick={() => setActiveFolderId('all')}
              >
                <span>{t.all}</span>
                <small>{commandTemplates.length}</small>
              </button>
              {grouped.map(({ folder, templates }) => (
                <button
                  key={folder.id}
                  className={activeFolderId === folder.id ? 'active' : ''}
                  type="button"
                  onClick={() => setActiveFolderId(folder.id)}
                >
                  <span>{folder.name}</span>
                  <small>{templates.length}</small>
                </button>
              ))}
              {ungrouped.length ? (
                <button
                  className={activeFolderId === 'ungrouped' ? 'active' : ''}
                  type="button"
                  onClick={() => setActiveFolderId('ungrouped')}
                >
                  <span>{t.commandUncategorized}</span>
                  <small>{ungrouped.length}</small>
                </button>
              ) : null}
            </div>
          </div>

          <div className="command-template-list scrollbar-scroll">
            <table className="fs-file-table compact command-table">
              <colgroup>
                <col style={{ width: '100%' }} />
              </colgroup>
              <thead>
                <tr>
                  <th>{t.name}</th>
                </tr>
              </thead>
              <tbody>
                {visibleTemplates.map((template) => (
                  <tr
                    key={template.id}
                    className={selectedTemplate?.id === template.id ? 'is-selected' : ''}
                    onClick={() => setSelectedCommandId(template.id)}
                  >
                    <td>
                      <div className="col-name-wrapper">
                        <strong>{template.name}</strong>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!visibleTemplates.length ? (
              <div className="command-empty-state">{t.commandEmpty}</div>
            ) : null}
          </div>
        </section>

        <div
          className="file-split-resizer"
          onMouseDown={() => {
            isResizingCommandSplit.current = true
            document.body.style.cursor = 'col-resize'
            document.body.style.userSelect = 'none'
          }}
          role="separator"
        />

        <section className="command-pane command-pane-preview">
          <div className="command-pane-head">
            <strong>{t.commandPreview}</strong>
            <span>{selectedTemplate ? t.commandRendered : t.commandNoDescription}</span>
          </div>

          <div className="command-runner scrollbar-scroll">
            {selectedTemplate ? (
              <>
                <div className="command-runner-head">
                  <div className="command-runner-title-line">
                    <strong>{selectedTemplate.name}</strong>
                    <SessionSendTargetPicker
                      allLabel={t.commandSendAllWithCount.replace('{count}', String(sendTargets.length))}
                      currentLabel={activeTab ? t.commandSendCurrentWithIndex.replace('{index}', String(sendTargets.find((target) => target.tabId === activeTab.id)?.index ?? '-')) : t.commandSendCurrent}
                      onScopeChange={setSendScope}
                      onSelectedTabIdsChange={setSelectedTabIds}
                      scope={sendScope}
                      selectedTabIds={selectedTabIds}
                      targets={sendTargets}
                      showRememberSelection={true}
                      rememberSelection={rememberSelection}
                      onRememberSelectionChange={setRememberSelection}
                      popover={true}
                    />
                  </div>
                  <div className="command-runner-actions">
                    <label className="command-toggle">
                      <input
                        checked={appendCarriageReturn}
                        type="checkbox"
                        onChange={(event) => setAppendCarriageReturn(event.currentTarget.checked)}
                      />
                      <span>{t.commandAppendCr}</span>
                    </label>
                    <button
                      type="button"
                      className="primary-button"
                      onClick={handleRun}
                      disabled={isBusy || (sendScope === 'current' ? !canRunCurrent : sendScope === 'all-ssh' ? !canRunAny : !canRunSelected)}
                    >
                      {t.send}
                    </button>
                  </div>
                </div>
                <div className="command-detail-block">
                  <span>{t.name}</span>
                  <p>{selectedTemplate.name}</p>
                </div>
                <div className="command-detail-block">
                  <span>{t.description}</span>
                  <p>{selectedTemplate.description || t.commandNoDescription}</p>
                </div>
                <div className="command-preview command-detail-block">
                  <span>{t.commandTemplate}</span>
                  <code>{selectedTemplate.command}</code>
                </div>
                {paramIndexes.length ? (
                  <div className="command-param-grid">
                    {paramIndexes.map((index) => (
                      <label key={index}>
                        <span>{`${t.commandParam} ${index}`}</span>
                        <input
                          type="text"
                          value={paramValues[index] ?? ''}
                          onChange={(event) => {
                            const value = event.currentTarget.value
                            setParamValues((prev) => ({ ...prev, [index]: value }))
                          }}
                        />
                      </label>
                    ))}
                  </div>
                ) : null}
                <div className="command-preview command-detail-block">
                  <span>{t.commandRendered}</span>
                  <code>{lastRenderedCommand || selectedTemplate.command}</code>
                </div>

              </>
            ) : (
              <div className="command-empty-state">{t.commandEmpty}</div>
            )}
          </div>
        </section>
      </div>
    </section>
  )
}
