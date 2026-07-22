import { useEffect, useMemo, useState, useRef } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  TerminalCommandHistoryEntry,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'
import { SessionSendTargetPicker } from '../common/SessionSendTargetPicker'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { VerticalScrollbar } from '../common/VerticalScrollbar'
import { WorkspaceLoadingState } from '../common/WorkspaceLoadingState'
import { CommandCodeEditor } from './CommandCodeEditor'
import { extractCommandParams, groupCommands, sortByOrder } from './command-utils'

const TEMPORARY_EDITOR_ID = '__temporary-command-editor__'
const TEMPORARY_HISTORY_LIMIT = 40

type TemporaryHistoryEntry = TerminalCommandHistoryEntry & {
  appendCarriageReturn: boolean
}

function temporaryHistoryKey(entry: TemporaryHistoryEntry) {
  return `${entry.createdAt}-${entry.command}`
}

function formatTemporaryHistoryTime(createdAt: number) {
  return new Date(createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function CommandCenter({
  activeTab,
  commandFolders,
  commandTemplates,
  isBusy,
  sendTargets,
  onExecute,
  onSendTerminalCommand,
  onSaveTemporaryCommand,
  onUpdateCommand,
  paneWidth,
  onPaneWidthChange
}: {
  activeTab: WorkspaceTab | null
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  sendTargets: SessionSendTarget[]
  onExecute(
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): void
  onSendTerminalCommand(
    command: string,
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): Promise<void>
  onSaveTemporaryCommand(command: string, appendCarriageReturn: boolean): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
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
  const [temporaryCommand, setTemporaryCommand] = useState('')
  const [isSendingTemporary, setIsSendingTemporary] = useState(false)
  const [temporaryHistory, setTemporaryHistory] = useState<TemporaryHistoryEntry[]>([])
  const [savingHistoryKey, setSavingHistoryKey] = useState<string | null>(null)
  const [paramValues, setParamValues] = useState<Record<number, string>>({})
  const [isEditingTemplate, setIsEditingTemplate] = useState(false)
  const [templateDraftCommand, setTemplateDraftCommand] = useState('')
  const [appendCarriageReturn, setAppendCarriageReturn] = useState(true)
  const [preferencesLoaded, setPreferencesLoaded] = useState(false)
  const [rememberSelection, setRememberSelection] = useState(false)
  const [sendScope, setSendScope] = useState<SendScope>('current')
  const [selectedTabIds, setSelectedTabIds] = useState<string[]>([])
  const isTemporaryEditor = activeFolderId === TEMPORARY_EDITOR_ID

  const splitRef = useRef<HTMLDivElement | null>(null)
  const templateListScrollRef = useRef<HTMLDivElement | null>(null)
  const isResizingCommandSplit = useRef(false)
  const temporaryHistoryRef = useRef<TemporaryHistoryEntry[]>([])

  const visibleTemplates = useMemo(() => {
    if (isTemporaryEditor) {
      return []
    }
    if (activeFolderId === 'all') {
      return sortByOrder(commandTemplates)
    }
    if (activeFolderId === 'ungrouped') {
      return ungrouped
    }
    return sortByOrder(commandTemplates.filter((template) => template.parentId === activeFolderId))
  }, [activeFolderId, commandTemplates, isTemporaryEditor, ungrouped])

  const selectedTemplate = useMemo(() => {
    if (isTemporaryEditor) {
      return null
    }
    return (
      visibleTemplates.find((template) => template.id === selectedCommandId) ??
      commandTemplates.find((template) => template.id === selectedCommandId) ??
      visibleTemplates[0] ??
      null
    )
  }, [commandTemplates, isTemporaryEditor, selectedCommandId, visibleTemplates])
  const paramIndexes = selectedTemplate ? extractCommandParams(selectedTemplate.command) : []
  const previewCommand = isEditingTemplate ? templateDraftCommand : (selectedTemplate?.command ?? '')
  const renderedCommand = useMemo(
    () =>
      previewCommand.replace(
        /\[p#(\d+)\]/g,
        (_, rawIndex: string) => paramValues[Number(rawIndex)] ?? `[p#${rawIndex}]`
      ),
    [paramValues, previewCommand]
  )
  const canRunCurrent = Boolean(
    activeTab && selectedTemplate && sendTargets.some((target) => target.tabId === activeTab.id)
  )
  const canRunAny = Boolean(sendTargets.length && selectedTemplate)
  const canRunSelected = Boolean(
    selectedTemplate && selectedTabIds.some((tabId) => sendTargets.some((target) => target.tabId === tabId))
  )
  const canSendTemporary = Boolean(
    temporaryCommand.trim() &&
    (sendScope === 'current'
      ? activeTab && sendTargets.some((target) => target.tabId === activeTab.id)
      : sendScope === 'all-ssh'
        ? sendTargets.length
        : selectedTabIds.some((tabId) => sendTargets.some((target) => target.tabId === tabId)))
  )

  useEffect(() => {
    if (!isTemporaryEditor && !selectedTemplate && commandTemplates[0]) {
      setSelectedCommandId(commandTemplates[0].id)
    }
  }, [commandTemplates, isTemporaryEditor, selectedTemplate])

  useEffect(() => {
    if (isTemporaryEditor) {
      return
    }
    setParamValues({})
    setAppendCarriageReturn(selectedTemplate?.appendCarriageReturn ?? true)
    setTemplateDraftCommand(selectedTemplate?.command ?? '')
    setIsEditingTemplate(false)
  }, [isTemporaryEditor, selectedTemplate?.command, selectedTemplate?.id])

  useEffect(() => {
    setSelectedTabIds((prev) => prev.filter((tabId) => sendTargets.some((target) => target.tabId === tabId)))
  }, [sendTargets])

  useEffect(() => {
    let canceled = false

    temporaryHistoryRef.current = []
    setTemporaryHistory([])

    if (!activeTab?.profileId || !window.fileterm?.getTerminalCommandHistory) {
      return
    }

    void window.fileterm
      .getTerminalCommandHistory(activeTab.profileId)
      .then((entries) => {
        if (canceled) {
          return
        }

        const hydratedEntries = entries.map((entry) => ({
          ...entry,
          appendCarriageReturn: true
        }))
        temporaryHistoryRef.current = hydratedEntries
        setTemporaryHistory(hydratedEntries)
      })
      .catch(() => {
        // History is an enhancement; the temporary editor remains usable when
        // stored history hydration fails.
      })

    return () => {
      canceled = true
    }
  }, [activeTab?.profileId])

  useEffect(() => {
    let canceled = false

    async function loadPreferences() {
      const desktopApi = window.fileterm
      if (!desktopApi?.getCommandSendPreferences) {
        setPreferencesLoaded(true)
        return
      }

      try {
        const storedPreferences = await desktopApi.getCommandSendPreferences()
        if (!canceled) {
          setRememberSelection(storedPreferences.rememberSelection)
          setSendScope(storedPreferences.rememberSelection ? storedPreferences.sendScope : 'current')
          setSelectedTabIds(storedPreferences.rememberSelection ? storedPreferences.selectedTabIds : [])
        }
      } catch {
        // Command execution remains usable when preference hydration fails.
      } finally {
        if (!canceled) {
          setPreferencesLoaded(true)
        }
      }
    }

    void loadPreferences()

    return () => {
      canceled = true
    }
  }, [])

  useEffect(() => {
    if (!preferencesLoaded || !window.fileterm?.setCommandSendPreferences) {
      return
    }

    void window.fileterm.setCommandSendPreferences({
      rememberSelection,
      sendScope: rememberSelection ? sendScope : 'current',
      selectedTabIds: rememberSelection ? selectedTabIds : []
    })
  }, [preferencesLoaded, rememberSelection, sendScope, selectedTabIds])

  const persistTemporaryHistory = (entries: TemporaryHistoryEntry[]) => {
    if (activeTab?.profileId && window.fileterm?.setTerminalCommandHistory) {
      void window.fileterm.setTerminalCommandHistory(
        activeTab.profileId,
        entries.map(({ command: historyCommand, createdAt }) => ({
          command: historyCommand,
          createdAt
        }))
      )
    }
  }

  const addTemporaryHistoryEntry = (command: string, nextAppendCarriageReturn: boolean) => {
    const entry = {
      command,
      createdAt: Date.now(),
      appendCarriageReturn: nextAppendCarriageReturn
    }
    const nextHistory = [entry, ...temporaryHistoryRef.current.filter((item) => item.command !== command)].slice(
      0,
      TEMPORARY_HISTORY_LIMIT
    )

    temporaryHistoryRef.current = nextHistory
    setTemporaryHistory(nextHistory)
    persistTemporaryHistory(nextHistory)
  }

  const handleTemporaryHistoryDelete = (entry: TemporaryHistoryEntry) => {
    const key = temporaryHistoryKey(entry)
    const nextHistory = temporaryHistoryRef.current.filter((item) => temporaryHistoryKey(item) !== key)
    temporaryHistoryRef.current = nextHistory
    setTemporaryHistory(nextHistory)
    persistTemporaryHistory(nextHistory)
  }

  const handleTemporaryHistoryClear = () => {
    temporaryHistoryRef.current = []
    setTemporaryHistory([])
    persistTemporaryHistory([])
  }

  const handleRun = () => {
    if (!selectedTemplate) {
      return
    }
    const args = paramIndexes.map((index) => paramValues[index] ?? '')
    onExecute(selectedTemplate.id, args, { appendCarriageReturn }, sendScope, selectedTabIds)
  }

  const handleTemplateEdit = () => {
    if (!selectedTemplate) return
    setTemplateDraftCommand(selectedTemplate.command)
    setIsEditingTemplate(true)
  }

  const handleTemplateSave = () => {
    if (!selectedTemplate || !isEditingTemplate) return
    const input: CommandTemplateInput = {
      name: selectedTemplate.name,
      command: templateDraftCommand,
      description: selectedTemplate.description,
      parentId: selectedTemplate.parentId,
      order: selectedTemplate.order,
      appendCarriageReturn
    }
    void Promise.resolve(onUpdateCommand(selectedTemplate.id, input)).then((saved) => {
      if (saved !== false) {
        setIsEditingTemplate(false)
      }
    })
  }

  const handleTemporaryRun = () => {
    if (isBusy || isSendingTemporary || !canSendTemporary) {
      return
    }
    const command = temporaryCommand.trim()
    setIsSendingTemporary(true)
    void onSendTerminalCommand(command, { appendCarriageReturn }, sendScope, selectedTabIds)
      .then(() => {
        addTemporaryHistoryEntry(command, appendCarriageReturn)
      })
      .catch(() => undefined)
      .finally(() => setIsSendingTemporary(false))
  }

  const handleTemporaryHistoryEdit = (entry: TemporaryHistoryEntry) => {
    setTemporaryCommand(entry.command)
    setAppendCarriageReturn(entry.appendCarriageReturn)
  }

  const handleTemporaryHistorySave = (entry: TemporaryHistoryEntry) => {
    const key = temporaryHistoryKey(entry)
    if (savingHistoryKey) {
      return
    }

    setSavingHistoryKey(key)
    void Promise.resolve(onSaveTemporaryCommand(entry.command, entry.appendCarriageReturn))
      .then(() => undefined)
      .catch(() => undefined)
      .finally(() => setSavingHistoryKey(null))
  }

  const handleTemporaryHistoryNew = () => {
    const command = temporaryCommand.trim()
    if (command) {
      addTemporaryHistoryEntry(command, appendCarriageReturn)
    }
    setTemporaryCommand('')
    setAppendCarriageReturn(true)
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
      {!preferencesLoaded ? <WorkspaceLoadingState label={t.loadingWorkspace} /> : null}
      {preferencesLoaded ? (
        <div
          className="command-center-body"
          ref={splitRef}
          style={{ '--list-pane-width': `${paneWidth}px` } as React.CSSProperties}
        >
          <section className="command-pane command-pane-list">
            <div className="command-folder-bar">
              <div className="command-folder-tabs-scroll" onWheel={handleHorizontalWheelScroll}>
                <div className="command-folder-tabs">
                  <button
                    className={`command-folder-tab-temporary ${isTemporaryEditor ? 'active' : ''}`}
                    type="button"
                    onClick={() => setActiveFolderId(TEMPORARY_EDITOR_ID)}
                  >
                    <span className="material-symbols-outlined" aria-hidden="true">
                      edit_note
                    </span>
                    <span>{t.commandTemporaryEditor}</span>
                  </button>
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
            </div>

            <div className="command-template-list-region">
              <div className="command-template-list scrollbar-scroll" ref={templateListScrollRef}>
                {isTemporaryEditor ? (
                  <div className="command-temporary-history">
                    <div className="command-temporary-history-head">
                      <div className="command-temporary-history-title">
                        <span className="material-symbols-outlined" aria-hidden="true">
                          history
                        </span>
                        <strong>{t.history}</strong>
                        <small>{temporaryHistory.length}</small>
                      </div>
                      <div className="command-temporary-history-actions">
                        <button
                          className="flat-button compact command-temporary-history-clear"
                          disabled={!temporaryHistory.length}
                          type="button"
                          title={t.clear}
                          onClick={handleTemporaryHistoryClear}
                        >
                          <span className="material-symbols-outlined" aria-hidden="true">
                            delete_sweep
                          </span>
                          <span>{t.clear}</span>
                        </button>
                        <button
                          className="flat-button compact command-temporary-history-new"
                          disabled={!temporaryCommand.trim()}
                          type="button"
                          aria-label={t.commandTemporaryHistoryNew}
                          title={t.commandTemporaryHistoryNew}
                          onClick={handleTemporaryHistoryNew}
                        >
                          <span className="material-symbols-outlined" aria-hidden="true">
                            add
                          </span>
                          <span>{t.commandTemporaryHistoryNew}</span>
                        </button>
                      </div>
                    </div>
                    <div className="command-temporary-history-help">{t.commandTemporaryHistoryNewHint}</div>
                    <div className="command-temporary-history-list scrollbar-scroll">
                      {temporaryHistory.length ? (
                        temporaryHistory.map((entry, index) => {
                          const key = temporaryHistoryKey(entry)
                          return (
                            <article className="command-temporary-history-item" key={key}>
                              <div className="command-temporary-history-command">
                                <span className="command-temporary-history-index" aria-hidden="true">
                                  {String(index + 1).padStart(2, '0')}
                                </span>
                                <code title={entry.command}>{entry.command}</code>
                              </div>
                              <div className="command-temporary-history-meta">
                                <time dateTime={new Date(entry.createdAt).toISOString()}>
                                  {formatTemporaryHistoryTime(entry.createdAt)}
                                </time>
                                <button
                                  className="command-temporary-history-action"
                                  type="button"
                                  title={t.edit}
                                  onClick={() => handleTemporaryHistoryEdit(entry)}
                                >
                                  <span className="material-symbols-outlined" aria-hidden="true">
                                    edit
                                  </span>
                                  <span>{t.edit}</span>
                                </button>
                                <button
                                  className="command-temporary-history-action"
                                  type="button"
                                  title={t.save}
                                  disabled={savingHistoryKey === key}
                                  onClick={() => handleTemporaryHistorySave(entry)}
                                >
                                  {savingHistoryKey === key ? (
                                    <span aria-hidden="true" className="button-spinner" />
                                  ) : (
                                    <span className="material-symbols-outlined" aria-hidden="true">
                                      save
                                    </span>
                                  )}
                                  <span>{t.save}</span>
                                </button>
                                <CloseButton
                                  aria-label={t.delete}
                                  className="command-temporary-history-delete"
                                  onClick={() => handleTemporaryHistoryDelete(entry)}
                                  size="tab"
                                  title={t.delete}
                                />
                              </div>
                            </article>
                          )
                        })
                      ) : (
                        <div className="command-temporary-history-empty">
                          <span className="material-symbols-outlined" aria-hidden="true">
                            history
                          </span>
                          <span>{t.commandTemporaryHistoryEmpty}</span>
                        </div>
                      )}
                    </div>
                  </div>
                ) : (
                  <>
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
                    {!visibleTemplates.length ? <div className="command-empty-state">{t.commandEmpty}</div> : null}
                  </>
                )}
              </div>
              <VerticalScrollbar ariaLabel={t.scrollCommandList} scrollRef={templateListScrollRef} topInset={24} />
            </div>
          </section>

          <div
            className="file-split-resizer"
            onMouseDown={(event) => {
              event.preventDefault()
              window.getSelection()?.removeAllRanges()
              isResizingCommandSplit.current = true
              document.body.style.cursor = 'col-resize'
              document.body.style.userSelect = 'none'
            }}
            role="separator"
          />

          <section className="command-pane command-pane-preview">
            <div
              className={`command-pane-head ${isTemporaryEditor ? 'command-temporary-pane-head' : 'command-template-pane-head'}`}
            >
              <strong>{isTemporaryEditor ? t.commandTemporaryEditorTitle : t.commandPreview}</strong>
              {isTemporaryEditor ? (
                <div className="command-pane-actions">
                  <label className="command-toggle">
                    <input
                      checked={appendCarriageReturn}
                      type="checkbox"
                      onChange={(event) => setAppendCarriageReturn(event.currentTarget.checked)}
                    />
                    <span>{t.commandAppendCr}</span>
                  </label>
                  <button
                    className="flat-button compact"
                    type="button"
                    onClick={() => setTemporaryCommand('')}
                    disabled={!temporaryCommand}
                  >
                    {t.clear}
                  </button>
                  <button
                    className="primary-button compact"
                    type="button"
                    onClick={handleTemporaryRun}
                    disabled={isBusy || isSendingTemporary || !canSendTemporary}
                  >
                    {isSendingTemporary ? <span aria-hidden="true" className="button-spinner" /> : null}
                    {t.send}
                  </button>
                  <SessionSendTargetPicker
                    allLabel={t.commandSendAllWithCount.replace('{count}', String(sendTargets.length))}
                    currentLabel={
                      activeTab
                        ? t.commandSendCurrentWithIndex.replace(
                            '{index}',
                            String(sendTargets.find((target) => target.tabId === activeTab.id)?.index ?? '-')
                          )
                        : t.commandSendCurrent
                    }
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
              ) : selectedTemplate ? (
                <div className="command-pane-actions">
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
                    className="primary-button compact"
                    onClick={handleRun}
                    disabled={
                      isBusy ||
                      (sendScope === 'current'
                        ? !canRunCurrent
                        : sendScope === 'all-ssh'
                          ? !canRunAny
                          : !canRunSelected)
                    }
                  >
                    {t.send}
                  </button>
                  <SessionSendTargetPicker
                    allLabel={t.commandSendAllWithCount.replace('{count}', String(sendTargets.length))}
                    currentLabel={
                      activeTab
                        ? t.commandSendCurrentWithIndex.replace(
                            '{index}',
                            String(sendTargets.find((target) => target.tabId === activeTab.id)?.index ?? '-')
                          )
                        : t.commandSendCurrent
                    }
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
              ) : null}
            </div>

            <div className={`command-runner scrollbar-scroll ${isTemporaryEditor ? 'command-temporary-runner' : ''}`}>
              {isTemporaryEditor ? (
                <div className="command-temporary-editor">
                  <div className="command-editor-field full command-editor-dialog-textarea command-temporary-editor-field">
                    <CommandCodeEditor
                      value={temporaryCommand}
                      onChange={setTemporaryCommand}
                      onKeyDown={(event) => {
                        if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
                          event.preventDefault()
                          handleTemporaryRun()
                        }
                      }}
                      placeholder={t.commandTemporaryEditorPlaceholder}
                      autoFocus={true}
                      ariaLabel={t.commandTemporaryEditorTitle}
                    />
                  </div>
                </div>
              ) : selectedTemplate ? (
                <>
                  <div className="command-runner-head command-template-runner-head">
                    <div className="command-runner-title-line">
                      <div className="command-template-description">
                        <span>{t.name}</span>
                        <strong>{selectedTemplate.name}</strong>
                        <span>{t.description}</span>
                        <p>{selectedTemplate.description || t.commandNoDescription}</p>
                      </div>
                      <div className="command-template-actions">
                        <button type="button" onClick={handleTemplateEdit} disabled={isEditingTemplate}>
                          <span className="material-symbols-outlined" aria-hidden="true">
                            edit
                          </span>
                          <span>{t.edit}</span>
                        </button>
                        <button type="button" onClick={handleTemplateSave} disabled={!isEditingTemplate || isBusy}>
                          <span className="material-symbols-outlined" aria-hidden="true">
                            save
                          </span>
                          <span>{t.save}</span>
                        </button>
                      </div>
                    </div>
                  </div>
                  <div className="command-preview command-detail-block command-template-preview">
                    <span>{t.commandTemplate}</span>
                    <div className="command-editor-dialog-textarea">
                      <CommandCodeEditor
                        value={previewCommand}
                        onChange={setTemplateDraftCommand}
                        readOnly={!isEditingTemplate}
                        ariaLabel={t.commandTemplate}
                      />
                    </div>
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
                  <div className="command-preview command-detail-block command-template-preview">
                    <span>{t.commandRendered}</span>
                    <div className="command-editor-dialog-textarea">
                      <CommandCodeEditor value={renderedCommand} readOnly={true} ariaLabel={t.commandRendered} />
                    </div>
                  </div>
                </>
              ) : (
                <div className="command-empty-state">{t.commandEmpty}</div>
              )}
            </div>
          </section>
        </div>
      ) : null}
    </section>
  )
}
