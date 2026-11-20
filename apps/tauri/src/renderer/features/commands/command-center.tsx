import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { useEffect, useMemo, useState, useRef } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  WorkspaceTab
} from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/close-button'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { WorkspaceLoadingState } from '../common/workspace-loading-state'
import { CommandPreviewPane } from './command-preview-pane'
import { CommandTemplateLibrary } from './command-template-library'
import { commandTemplatesForFolder, extractCommandParams, sortByOrder } from './command-utils'
import {
  TEMPORARY_EDITOR_ID,
  TEMPORARY_HISTORY_LIMIT,
  formatTemporaryHistoryTime,
  temporaryHistoryKey,
  type TemporaryHistoryEntry
} from './command-center-utils'

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
  const [activeFolderId, setActiveFolderId] = useState<string>('all')
  const [selectedCommandId, setSelectedCommandId] = useState<string | null>(() => {
    const initialSorted = sortByOrder(commandTemplates)
    return initialSorted[0]?.id ?? null
  })
  const [temporaryCommand, setTemporaryCommand] = useState('')
  const [isSendingTemporary, setIsSendingTemporary] = useState(false)
  const [temporaryHistory, setTemporaryHistory] = useState<TemporaryHistoryEntry[]>([])
  const [isSavingToCommandManager, setIsSavingToCommandManager] = useState(false)
  const [selectedTemporaryHistoryKey, setSelectedTemporaryHistoryKey] = useState<string | null>(null)
  const [isEditingTemporary, setIsEditingTemporary] = useState(true)
  const [temporaryEditorVersion, setTemporaryEditorVersion] = useState(0)
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
  const isResizingCommandSplit = useRef(false)
  const temporaryHistoryRef = useRef<TemporaryHistoryEntry[]>([])

  const visibleTemplates = useMemo(
    () => (isTemporaryEditor ? [] : commandTemplatesForFolder(commandTemplates, activeFolderId)),
    [activeFolderId, commandTemplates, isTemporaryEditor]
  )

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
  const previewCommand = useMemo(() => {
    if (isEditingTemplate) {
      return templateDraftCommand
    }
    const rawCommand = selectedTemplate?.command ?? ''
    if (!paramIndexes.length) {
      return rawCommand
    }
    let interpolated = rawCommand
    for (const index of paramIndexes) {
      const value = paramValues[index]
      if (value !== undefined && value !== '') {
        interpolated = interpolated.replaceAll(`[p#${index}]`, value)
      }
    }
    return interpolated
  }, [isEditingTemplate, templateDraftCommand, selectedTemplate?.command, paramIndexes, paramValues])
  const selectedTemporaryHistory = useMemo(
    () => temporaryHistory.find((entry) => temporaryHistoryKey(entry) === selectedTemporaryHistoryKey) ?? null,
    [selectedTemporaryHistoryKey, temporaryHistory]
  )
  const isTemporaryPreview = Boolean(selectedTemporaryHistory && !isEditingTemporary)
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
    if (isTemporaryEditor) return
    if (!visibleTemplates.length) {
      setSelectedCommandId(null)
      return
    }
    if (!selectedCommandId || !visibleTemplates.some((template) => template.id === selectedCommandId)) {
      setSelectedCommandId(visibleTemplates[0].id)
    }
  }, [isTemporaryEditor, visibleTemplates, selectedCommandId])

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
    setSelectedTemporaryHistoryKey(null)
    setTemporaryCommand('')
    setAppendCarriageReturn(true)
    setIsEditingTemporary(true)

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

  const addTemporaryHistoryEntry = (command: string, nextAppendCarriageReturn: boolean, replacedEntryKey?: string) => {
    const entry = {
      command,
      createdAt: Date.now(),
      appendCarriageReturn: nextAppendCarriageReturn
    }
    const nextHistory = [
      entry,
      ...temporaryHistoryRef.current.filter(
        (item) => item.command !== command && temporaryHistoryKey(item) !== replacedEntryKey
      )
    ].slice(0, TEMPORARY_HISTORY_LIMIT)

    temporaryHistoryRef.current = nextHistory
    setTemporaryHistory(nextHistory)
    persistTemporaryHistory(nextHistory)
    return entry
  }

  const handleTemporaryHistoryDelete = (entry: TemporaryHistoryEntry) => {
    const key = temporaryHistoryKey(entry)
    const nextHistory = temporaryHistoryRef.current.filter((item) => temporaryHistoryKey(item) !== key)
    temporaryHistoryRef.current = nextHistory
    setTemporaryHistory(nextHistory)
    persistTemporaryHistory(nextHistory)
    if (selectedTemporaryHistoryKey === key) {
      setSelectedTemporaryHistoryKey(null)
      setTemporaryCommand('')
      setAppendCarriageReturn(true)
      setIsEditingTemporary(true)
      setTemporaryEditorVersion((version) => version + 1)
    }
  }

  const handleTemporaryHistoryClear = () => {
    temporaryHistoryRef.current = []
    setTemporaryHistory([])
    persistTemporaryHistory([])
    setSelectedTemporaryHistoryKey(null)
    setTemporaryCommand('')
    setAppendCarriageReturn(true)
    setIsEditingTemporary(true)
    setTemporaryEditorVersion((version) => version + 1)
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

  const handleTemporaryHistorySelect = (entry: TemporaryHistoryEntry) => {
    setSelectedTemporaryHistoryKey(temporaryHistoryKey(entry))
    setTemporaryCommand(entry.command)
    setAppendCarriageReturn(entry.appendCarriageReturn)
    setIsEditingTemporary(false)
  }

  const handleTemporaryEdit = () => {
    if (!selectedTemporaryHistory) {
      return
    }
    setIsEditingTemporary(true)
    setTemporaryEditorVersion((version) => version + 1)
  }

  const handleTemporarySave = () => {
    const command = temporaryCommand.trim()
    if (!command) {
      return
    }

    const savedEntry = addTemporaryHistoryEntry(command, appendCarriageReturn, selectedTemporaryHistoryKey ?? undefined)
    setSelectedTemporaryHistoryKey(temporaryHistoryKey(savedEntry))
    setIsEditingTemporary(false)
  }

  const handleSaveTemporaryToCommandManager = () => {
    const command = temporaryCommand.trim()
    if (!command || isSavingToCommandManager) {
      return
    }

    setIsSavingToCommandManager(true)
    void Promise.resolve(onSaveTemporaryCommand(command, appendCarriageReturn))
      .then(() => undefined)
      .catch(() => undefined)
      .finally(() => setIsSavingToCommandManager(false))
  }

  const handleTemporaryHistoryNew = () => {
    setTemporaryCommand('')
    setAppendCarriageReturn(true)
    setSelectedTemporaryHistoryKey(null)
    setIsEditingTemporary(true)
    setTemporaryEditorVersion((version) => version + 1)
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
          <CommandTemplateLibrary
            folderId={activeFolderId}
            folders={commandFolders}
            leadingTab={
              <button
                className={`command-folder-tab-temporary ${isTemporaryEditor ? 'active' : ''}`}
                type="button"
                onClick={() => setActiveFolderId(TEMPORARY_EDITOR_ID)}
              >
                <MaterialIcon name="edit_note" className="material-symbols-outlined" aria-hidden="true" />
                <span>{t.commandTemporaryEditor}</span>
              </button>
            }
            onFolderChange={setActiveFolderId}
            onSelect={setSelectedCommandId}
            selectedId={selectedTemplate?.id}
            templates={commandTemplates}
            visibleTemplates={visibleTemplates}
          >
            {isTemporaryEditor ? (
              <div className="command-temporary-history">
                <div className="command-temporary-history-head">
                  <div className="command-temporary-history-title">
                    <MaterialIcon name="history" className="material-symbols-outlined" aria-hidden="true" />
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
                      <MaterialIcon name="delete_sweep" className="material-symbols-outlined" aria-hidden="true" />
                      <span>{t.clear}</span>
                    </button>
                    <button
                      className="flat-button compact command-temporary-history-new"
                      type="button"
                      aria-label={t.commandTemporaryHistoryNew}
                      title={t.commandTemporaryHistoryNew}
                      onClick={handleTemporaryHistoryNew}
                    >
                      <MaterialIcon name="add" className="material-symbols-outlined" aria-hidden="true" />
                      <span>{t.commandTemporaryHistoryNew}</span>
                    </button>
                  </div>
                </div>
                <div className="command-temporary-history-list scrollbar-scroll">
                  {temporaryHistory.length ? (
                    temporaryHistory.map((entry) => {
                      const key = temporaryHistoryKey(entry)
                      return (
                        <article
                          aria-pressed={selectedTemporaryHistoryKey === key}
                          className={`command-temporary-history-item ${selectedTemporaryHistoryKey === key ? 'is-selected' : ''}`}
                          key={key}
                          role="button"
                          tabIndex={0}
                          onClick={() => handleTemporaryHistorySelect(entry)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault()
                              handleTemporaryHistorySelect(entry)
                            }
                          }}
                        >
                          <time dateTime={new Date(entry.createdAt).toISOString()}>
                            {formatTemporaryHistoryTime(entry.createdAt)}
                          </time>
                          <code title={entry.command}>{entry.command}</code>
                          <CloseButton
                            aria-label={t.delete}
                            className="command-temporary-history-delete"
                            onClick={(event) => {
                              event.stopPropagation()
                              handleTemporaryHistoryDelete(entry)
                            }}
                            size="tab"
                            title={t.delete}
                          />
                        </article>
                      )
                    })
                  ) : (
                    <div className="command-temporary-history-empty">
                      <MaterialIcon name="history" className="material-symbols-outlined" aria-hidden="true" />
                      <span>{t.commandTemporaryHistoryEmpty}</span>
                    </div>
                  )}
                </div>
              </div>
            ) : null}
          </CommandTemplateLibrary>

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

          <CommandPreviewPane
            isTemporaryEditor={isTemporaryEditor}
            hasSelectedTemporaryHistory={Boolean(selectedTemporaryHistory)}
            isEditingTemporary={isEditingTemporary}
            temporaryCommand={temporaryCommand}
            isSavingToCommandManager={isSavingToCommandManager}
            isSendingTemporary={isSendingTemporary}
            canSendTemporary={canSendTemporary}
            appendCarriageReturn={appendCarriageReturn}
            activeTab={activeTab}
            sendTargets={sendTargets}
            sendScope={sendScope}
            selectedTabIds={selectedTabIds}
            rememberSelection={rememberSelection}
            selectedTemplate={selectedTemplate}
            isEditingTemplate={isEditingTemplate}
            isBusy={isBusy}
            previewCommand={previewCommand}
            paramIndexes={paramIndexes}
            paramValues={paramValues}
            temporaryEditorVersion={temporaryEditorVersion}
            isTemporaryPreview={isTemporaryPreview}
            canRunCurrent={canRunCurrent}
            canRunAny={canRunAny}
            canRunSelected={canRunSelected}
            onTemporaryEdit={handleTemporaryEdit}
            onTemporarySave={handleTemporarySave}
            onSaveTemporaryToCommandManager={handleSaveTemporaryToCommandManager}
            onAppendCarriageReturnChange={setAppendCarriageReturn}
            onTemporaryClear={() => {
              setTemporaryCommand('')
              setSelectedTemporaryHistoryKey(null)
              setIsEditingTemporary(true)
              setTemporaryEditorVersion((version) => version + 1)
            }}
            onTemporaryRun={handleTemporaryRun}
            onScopeChange={setSendScope}
            onSelectedTabIdsChange={setSelectedTabIds}
            onRememberSelectionChange={setRememberSelection}
            onTemplateEdit={handleTemplateEdit}
            onTemplateSave={handleTemplateSave}
            onRun={handleRun}
            onTemporaryCommandChange={setTemporaryCommand}
            onTemplateDraftCommandChange={setTemplateDraftCommand}
            onParamValueChange={(index, value) => setParamValues((prev) => ({ ...prev, [index]: value }))}
            onClearParamValue={(index) => setParamValues((prev) => ({ ...prev, [index]: '' }))}
          />
        </div>
      ) : null}
    </section>
  )
}
