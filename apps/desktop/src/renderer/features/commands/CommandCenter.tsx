import { useEffect, useMemo, useState } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  WorkspaceTab
} from '@termdock/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { extractCommandParams, groupCommands, sortByOrder } from './command-utils'

type SendScope = 'current' | 'all-ssh'

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
  tabs,
  onExecute,
}: {
  activeTab: WorkspaceTab | null
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  tabs: WorkspaceTab[]
  onExecute(commandId: string, args: string[], options: CommandExecutionOptions, scope: SendScope): void
}) {
  const grouped = useMemo(() => groupCommands(commandFolders, commandTemplates), [commandFolders, commandTemplates])
  const ungrouped = useMemo(
    () => sortByOrder(commandTemplates.filter((template) => !template.parentId)),
    [commandTemplates]
  )
  const sshTabs = useMemo(
    () => tabs.filter((tab) => tab.sessionType === 'ssh' && tab.status !== 'closed'),
    [tabs]
  )
  const [activeFolderId, setActiveFolderId] = useState<string>('all')
  const [selectedCommandId, setSelectedCommandId] = useState<string | null>(commandTemplates[0]?.id ?? null)
  const [paramValues, setParamValues] = useState<Record<number, string>>({})
  const [lastRenderedCommand, setLastRenderedCommand] = useState('')
  const [appendCarriageReturn, setAppendCarriageReturn] = useState(true)
  const [sendScope, setSendScope] = useState<SendScope>('current')

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
  const canRunCurrent = Boolean(activeTab && activeTab.sessionType === 'ssh' && selectedTemplate)
  const canRunAny = Boolean(sshTabs.length && selectedTemplate)

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

  const handleRun = () => {
    if (!selectedTemplate) {
      return
    }
    const args = paramIndexes.map((index) => paramValues[index] ?? '')
    const rendered = selectedTemplate.command.replace(/\[p#(\d+)\]/g, (_, rawIndex: string) => args[Number(rawIndex) - 1] ?? '')
    setLastRenderedCommand(rendered)
    onExecute(selectedTemplate.id, args, { appendCarriageReturn }, sendScope)
  }
  return (
    <section className="command-center">
      <div className="command-center-body">
        <section className="command-pane command-pane-list">
          <div className="command-folder-bar">
            <div className="command-folder-tabs">
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

          <div className="command-template-list">
            <table className="command-table">
              <thead>
                <tr>
                  <th className="col-name">{t.name}</th>
                  <th className="col-template">{t.commandTemplate}</th>
                </tr>
              </thead>
              <tbody>
                {visibleTemplates.map((template) => (
                  <tr
                    key={template.id}
                    className={selectedTemplate?.id === template.id ? 'active' : ''}
                    onClick={() => setSelectedCommandId(template.id)}
                  >
                    <td className="col-name">
                      <span className="command-icon">
                        <AppIcon name="flash" size={14} />
                      </span>
                      <strong>{template.name}</strong>
                    </td>
                    <td className="col-template">
                      <span className="command-template-inline">{getCommandSummary(template)}</span>
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

        <section className="command-pane command-pane-preview">
          <div className="command-pane-head">
            <strong>{t.commandPreview}</strong>
            <span>{selectedTemplate ? t.commandRendered : t.commandNoDescription}</span>
          </div>

          <div className="command-runner">
            {selectedTemplate ? (
              <>
                <div className="command-runner-head">
                  <strong>{selectedTemplate.name}</strong>
                  <button type="button" onClick={handleRun} disabled={isBusy || (sendScope === 'current' ? !canRunCurrent : !canRunAny)}>
                    <AppIcon name="flash" />
                    {t.send}
                  </button>
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
                <div className="command-runner-controls">
                  <label className="command-toggle">
                    <input
                      checked={appendCarriageReturn}
                      type="checkbox"
                      onChange={(event) => setAppendCarriageReturn(event.currentTarget.checked)}
                    />
                    <span>{t.commandAppendCr}</span>
                  </label>
                  <label className="command-target-select">
                    <span>{t.commandSendScope}</span>
                    <select value={sendScope} onChange={(event) => setSendScope(event.currentTarget.value as SendScope)}>
                      <option value="current">{t.commandSendCurrent}</option>
                      <option value="all-ssh">{t.commandSendAll}</option>
                    </select>
                  </label>
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
