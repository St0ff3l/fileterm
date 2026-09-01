import type { CommandTemplate, WorkspaceTab } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { handleHorizontalWheelScroll } from '../common/horizontal-scroll'
import { SelectionControl } from '../common/selection-control'
import { SessionSendTargetPicker } from '../common/session-send-target-picker'
import { StableButtonContent } from '../common/stable-button-content'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { CommandCodeEditor } from './command-code-editor'

export function CommandPreviewPane({
  isTemporaryEditor,
  hasSelectedTemporaryHistory,
  isEditingTemporary,
  temporaryCommand,
  isSavingToCommandManager,
  isSendingTemporary,
  canSendTemporary,
  appendCarriageReturn,
  activeTab,
  sendTargets,
  sendScope,
  selectedTabIds,
  rememberSelection,
  selectedTemplate,
  isEditingTemplate,
  isBusy,
  previewCommand,
  paramIndexes,
  paramValues,
  temporaryEditorVersion,
  isTemporaryPreview,
  canRunCurrent,
  canRunAny,
  canRunSelected,
  onTemporaryEdit,
  onTemporarySave,
  onSaveTemporaryToCommandManager,
  onAppendCarriageReturnChange,
  onTemporaryClear,
  onTemporaryRun,
  onScopeChange,
  onSelectedTabIdsChange,
  onRememberSelectionChange,
  onTemplateEdit,
  onTemplateSave,
  onRun,
  onTemporaryCommandChange,
  onTemplateDraftCommandChange,
  onParamValueChange,
  onClearParamValue
}: {
  isTemporaryEditor: boolean
  hasSelectedTemporaryHistory: boolean
  isEditingTemporary: boolean
  temporaryCommand: string
  isSavingToCommandManager: boolean
  isSendingTemporary: boolean
  canSendTemporary: boolean
  appendCarriageReturn: boolean
  activeTab: WorkspaceTab | null
  sendTargets: SessionSendTarget[]
  sendScope: SendScope
  selectedTabIds: string[]
  rememberSelection: boolean
  selectedTemplate: CommandTemplate | null
  isEditingTemplate: boolean
  isBusy: boolean
  previewCommand: string
  paramIndexes: number[]
  paramValues: Record<number, string>
  temporaryEditorVersion: number
  isTemporaryPreview: boolean
  canRunCurrent: boolean
  canRunAny: boolean
  canRunSelected: boolean
  onTemporaryEdit(): void
  onTemporarySave(): void
  onSaveTemporaryToCommandManager(): void
  onAppendCarriageReturnChange(value: boolean): void
  onTemporaryClear(): void
  onTemporaryRun(): void
  onScopeChange(scope: SendScope): void
  onSelectedTabIdsChange(tabIds: string[]): void
  onRememberSelectionChange(value: boolean): void
  onTemplateEdit(): void
  onTemplateSave(): void
  onRun(): void
  onTemporaryCommandChange(value: string): void
  onTemplateDraftCommandChange(value: string): void
  onParamValueChange(index: number, value: string): void
  onClearParamValue(index: number): void
}) {
  const currentLabel = activeTab
    ? t.commandSendCurrentWithIndex.replace(
        '{index}',
        String(sendTargets.find((target) => target.tabId === activeTab.id)?.index ?? '-')
      )
    : t.commandSendCurrent
  const allLabel = t.commandSendAllWithCount.replace('{count}', String(sendTargets.length))

  return (
    <section className="command-pane command-pane-preview">
      <div
        className={`command-pane-head ${isTemporaryEditor ? 'command-temporary-pane-head' : 'command-template-pane-head'}`}
      >
        <strong>{isTemporaryEditor ? t.commandTemporaryEditorTitle : t.commandPreview}</strong>
        {isTemporaryEditor ? (
          <>
            <div className="command-pane-edit-actions">
              <button
                disabled={!hasSelectedTemporaryHistory || isEditingTemporary}
                type="button"
                onClick={onTemporaryEdit}
              >
                <span className="material-symbols-outlined" aria-hidden="true">
                  edit
                </span>
                <span>{t.edit}</span>
              </button>
              <button disabled={!temporaryCommand.trim()} type="button" onClick={onTemporarySave}>
                <span className="material-symbols-outlined" aria-hidden="true">
                  save
                </span>
                <span>{t.save}</span>
              </button>
              <button
                aria-busy={isSavingToCommandManager}
                disabled={!temporaryCommand.trim() || isSavingToCommandManager}
                type="button"
                onClick={onSaveTemporaryToCommandManager}
              >
                <StableButtonContent
                  busy={isSavingToCommandManager}
                  icon={
                    <span className="material-symbols-outlined" aria-hidden="true">
                      save_as
                    </span>
                  }
                  label={t.commandSaveToManager}
                />
              </button>
            </div>
            <div className="command-pane-actions">
              <label className="command-toggle">
                <SelectionControl
                  checked={appendCarriageReturn}
                  type="checkbox"
                  onChange={(event) => onAppendCarriageReturnChange(event.currentTarget.checked)}
                />
                <span>{t.commandAppendCr}</span>
              </label>
              <button
                className="flat-button compact"
                type="button"
                onClick={onTemporaryClear}
                disabled={!temporaryCommand}
              >
                {t.clear}
              </button>
              <button
                aria-busy={isSendingTemporary}
                className="primary-button compact"
                type="button"
                onClick={onTemporaryRun}
                disabled={isBusy || isSendingTemporary || !canSendTemporary}
              >
                <StableButtonContent busy={isSendingTemporary} label={t.send} />
              </button>
              <SessionSendTargetPicker
                allLabel={allLabel}
                currentLabel={currentLabel}
                onScopeChange={onScopeChange}
                onSelectedTabIdsChange={onSelectedTabIdsChange}
                scope={sendScope}
                selectedTabIds={selectedTabIds}
                targets={sendTargets}
                showRememberSelection={true}
                rememberSelection={rememberSelection}
                onRememberSelectionChange={onRememberSelectionChange}
                popover={true}
              />
            </div>
          </>
        ) : selectedTemplate ? (
          <>
            <div className="command-pane-edit-actions">
              <button type="button" onClick={onTemplateEdit} disabled={isEditingTemplate}>
                <span className="material-symbols-outlined" aria-hidden="true">
                  edit
                </span>
                <span>{t.edit}</span>
              </button>
              <button type="button" onClick={onTemplateSave} disabled={!isEditingTemplate || isBusy}>
                <span className="material-symbols-outlined" aria-hidden="true">
                  save
                </span>
                <span>{t.save}</span>
              </button>
            </div>
            <div className="command-template-description command-pane-template-description">
              <span>{t.description}</span>
              <p>{selectedTemplate.description || t.commandNoDescription}</p>
            </div>
            <div className="command-pane-actions">
              <label className="command-toggle">
                <SelectionControl
                  checked={appendCarriageReturn}
                  type="checkbox"
                  onChange={(event) => onAppendCarriageReturnChange(event.currentTarget.checked)}
                />
                <span>{t.commandAppendCr}</span>
              </label>
              <button
                type="button"
                className="primary-button compact"
                onClick={onRun}
                disabled={
                  isBusy ||
                  (sendScope === 'current' ? !canRunCurrent : sendScope === 'all-ssh' ? !canRunAny : !canRunSelected)
                }
              >
                {t.send}
              </button>
              <SessionSendTargetPicker
                allLabel={allLabel}
                currentLabel={currentLabel}
                onScopeChange={onScopeChange}
                onSelectedTabIdsChange={onSelectedTabIdsChange}
                scope={sendScope}
                selectedTabIds={selectedTabIds}
                targets={sendTargets}
                showRememberSelection={true}
                rememberSelection={rememberSelection}
                onRememberSelectionChange={onRememberSelectionChange}
                popover={true}
              />
            </div>
          </>
        ) : null}
      </div>

      <div className={`command-runner scrollbar-scroll ${isTemporaryEditor ? 'command-temporary-runner' : ''}`}>
        {isTemporaryEditor ? (
          <div className="command-temporary-editor">
            <div className="command-editor-field full command-editor-dialog-textarea command-temporary-editor-field">
              <CommandCodeEditor
                key={temporaryEditorVersion}
                value={temporaryCommand}
                onChange={onTemporaryCommandChange}
                onKeyDown={(event) => {
                  if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
                    event.preventDefault()
                    onTemporaryRun()
                  }
                }}
                placeholder={t.commandTemporaryEditorPlaceholder}
                autoFocus={isEditingTemporary}
                ariaLabel={t.commandTemporaryEditorTitle}
                readOnly={isTemporaryPreview}
              />
            </div>
          </div>
        ) : selectedTemplate ? (
          <div className="command-preview command-detail-block command-template-preview">
            <div className="command-editor-dialog-textarea">
              <CommandCodeEditor
                value={previewCommand}
                onChange={onTemplateDraftCommandChange}
                readOnly={!isEditingTemplate}
                ariaLabel={t.commandTemplate}
              />
            </div>
            {paramIndexes.length ? (
              <div className="command-param-docked-bar" role="toolbar" aria-label={t.commandParam}>
                <div className="command-param-docked-lead" title={`${t.commandParam} (${paramIndexes.length})`}>
                  <AppIcon name="code" size={11} />
                </div>
                <div className="command-param-scroll-track" onWheel={handleHorizontalWheelScroll}>
                  {paramIndexes.map((index) => (
                    <div key={index} className="command-param-chip">
                      <span className="command-param-chip-prefix">
                        <span className="command-param-chip-tag">{`p#${index}`}</span>
                      </span>
                      <input
                        type="text"
                        value={paramValues[index] ?? ''}
                        placeholder={`[p#${index}]`}
                        title={
                          paramValues[index] ? `${t.commandParam} ${index}: ${paramValues[index]}` : `[p#${index}]`
                        }
                        aria-label={`${t.commandParam} ${index}`}
                        onChange={(event) => onParamValueChange(index, event.currentTarget.value)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' && !event.shiftKey) {
                            event.preventDefault()
                            onRun()
                          }
                        }}
                      />
                      {paramValues[index] ? (
                        <button
                          type="button"
                          className="command-param-chip-clear"
                          title={t.clear}
                          aria-label={t.clear}
                          tabIndex={-1}
                          onClick={() => onClearParamValue(index)}
                        >
                          <AppIcon name="close" size={9} />
                        </button>
                      ) : null}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="command-empty-state">
            <span>{t.commandEmpty}</span>
          </div>
        )}
      </div>
    </section>
  )
}
