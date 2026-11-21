import type { CommandBatchRun, CommandBatchTarget, CommandFolder, CommandTemplate } from '@fileterm/core'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Button } from '../../components/common/button/button'
import { Dialog } from '../../components/common/dialog/dialog'
import { FormDialogBody } from '../../components/common/dialog/form-dialog-body'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../components/common/input/input'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { formatMessage, t } from '../../i18n'
import { CommandCodeEditor } from '../commands/command-code-editor'
import { CommandTemplateLibrary } from '../commands/command-template-library'
import { commandTemplatesForFolder, extractCommandParams } from '../commands/command-utils'
import { AppIcon } from '../common/app-icon'
import './server-command-batch-dialog.css'

export type ServerCommandBatchDialogMode =
  | { type: 'run'; targets: Array<CommandBatchTarget & { name: string; address: string }> }
  | { type: 'history' }
  | { type: 'prepare'; message: string; connect?: { label: string; onConnect(): void } }

export function ServerCommandBatchDialog({
  mode,
  folders,
  templates,
  onClose
}: {
  mode: ServerCommandBatchDialogMode
  folders: CommandFolder[]
  templates: CommandTemplate[]
  onClose(): void
}) {
  const desktopApi = window.fileterm
  const [folderId, setFolderId] = useState('all')
  const [templateId, setTemplateId] = useState('')
  const [args, setArgs] = useState<Record<number, string>>({})
  const [run, setRun] = useState<CommandBatchRun | null>(null)
  const [history, setHistory] = useState<CommandBatchRun[] | null>(null)
  const [historyId, setHistoryId] = useState('')
  const [running, setRunning] = useState(false)
  const [error, setError] = useState('')
  const submitting = useRef(false)
  const browsing = mode.type === 'history'
  const compact = mode.type === 'prepare' || (browsing && !history?.length)
  const connect = mode.type === 'prepare' ? mode.connect : undefined
  const selectedRun = browsing ? history?.find(({ id }) => id === historyId) : run
  const visibleTemplates = commandTemplatesForFolder(templates, folderId)
  const template = visibleTemplates.find(({ id }) => id === templateId) ?? visibleTemplates[0]
  const preview =
    template?.command.replace(/\[p#(\d+)\]/g, (token, index: string) => args[Number(index)] ?? token) ?? ''
  const params = template ? extractCommandParams(template.command).filter((i) => Number.isSafeInteger(i) && i > 0) : []
  const canRun = mode.type === 'run' && Boolean(template?.command.trim()) && Boolean(desktopApi) && !run
  const title = browsing ? t.serverBatchHistory : run ? t.serverBatchResults : t.serverBatchRun

  useEffect(() => {
    if (!browsing) return
    if (!desktopApi) {
      setHistory([])
      return
    }
    let active = true
    void desktopApi.listCommandBatches().then(
      (entries) => {
        if (!active) return
        setHistory(entries)
        setHistoryId(entries[0]?.id ?? '')
      },
      (reason) => {
        if (active) setError(String(reason))
      }
    )
    return () => {
      active = false
    }
  }, [browsing, desktopApi])

  const start = async () => {
    if (mode.type !== 'run' || !desktopApi || !template || !canRun || submitting.current) return
    submitting.current = true
    setRunning(true)
    setError('')
    try {
      const targets = mode.targets.map(({ profileId, tabId, sessionRevision }) => ({
        profileId,
        tabId,
        sessionRevision
      }))
      setRun(await desktopApi.runCommandBatch(template.id, template.command, args, targets))
    } catch (reason) {
      setError(String(reason))
    } finally {
      submitting.current = false
      setRunning(false)
    }
  }

  const close = () => {
    if (!submitting.current) onClose()
  }

  const statuses = {
    success: t.serverBatchSuccess,
    failed: t.serverBatchFailed,
    timedOut: t.serverBatchTimeout,
    inputRequired: t.serverBatchInputRequired,
    error: t.serverBatchError,
    unknown: t.serverBatchUnknown
  }

  return createPortal(
    <Dialog
      aria-label={title}
      aria-busy={running}
      className="server-command-batch-dialog"
      closeOnBackdropClick={!running}
      closeOnEscape={!running}
      footer={
        mode.type === 'run' && !run ? (
          <>
            <Button disabled={running} onClick={close} size="panel">
              {t.cancel}
            </Button>
            <Button disabled={!canRun} loading={running} onClick={() => void start()} size="panel" variant="primary">
              {t.serverBatchRun}
            </Button>
          </>
        ) : connect ? (
          <Button
            onClick={() => {
              connect.onConnect()
              close()
            }}
            size="panel"
            variant="primary"
          >
            {connect.label}
          </Button>
        ) : undefined
      }
      isOpen
      onClose={close}
      showCloseButton={!running}
      size={compact ? 'sm' : mode.type === 'run' && !run ? 'lg' : 'md'}
      title={title}
    >
      <FormDialogBody>
        <div className="server-command-batch-content">
          {error && <p role="alert">{error}</p>}
          {running && <p role="status">{t.serverBatchRunning}</p>}
          {mode.type === 'prepare' && (
            <div className="server-command-batch-message">
              <MaterialIcon aria-hidden="true" name="dns" />
              <p>{mode.message}</p>
            </div>
          )}
          {mode.type === 'run' && !run && (
            <>
              <div className="server-command-batch-picker">
                <CommandTemplateLibrary
                  disabled={running}
                  folderId={folderId}
                  folders={folders}
                  onFolderChange={(id) => {
                    if (id === folderId) return
                    setArgs({})
                    setFolderId(id)
                  }}
                  onSelect={(id) => {
                    if (id === template?.id) return
                    setArgs({})
                    setTemplateId(id)
                  }}
                  selectedId={template?.id}
                  templates={templates}
                  visibleTemplates={visibleTemplates}
                />
                <section aria-label={t.commandPreview} className="command-pane command-pane-preview">
                  <div className="command-pane-head">
                    <strong>{t.commandPreview}</strong>
                  </div>
                  <div className="command-editor-dialog-textarea">
                    <CommandCodeEditor ariaLabel={t.commandPreview} readOnly value={preview} />
                  </div>
                </section>
              </div>
              {params.map((index) => (
                <label className="server-command-batch-field" key={index}>
                  {t.commandParam} {index}
                  <Input
                    disabled={running}
                    onChange={(event) => {
                      const value = event.target.value
                      setArgs((current) => ({ ...current, [index]: value }))
                    }}
                    value={args[index] ?? ''}
                  />
                </label>
              ))}
              <div className="server-command-batch-field">
                <span>{formatMessage(t.serverBatchTargets, { count: mode.targets.length })}</span>
                <ul className="server-command-batch-targets">
                  {mode.targets.map(({ profileId, name, address }) => (
                    <li key={profileId}>
                      <MaterialIcon aria-hidden="true" name="dns" />
                      <strong>{name}</strong>
                      <small>{address}</small>
                    </li>
                  ))}
                </ul>
              </div>
            </>
          )}
          {browsing &&
            !error &&
            (history?.length ? (
              <>
                <DropdownSelect
                  ariaLabel={t.serverBatchHistory}
                  className="server-command-batch-history-select"
                  menuClassName="server-command-batch-history-menu"
                  menuPlacement="auto"
                  onChange={setHistoryId}
                  options={history.map(({ id, startedAt, templateName }) => ({
                    value: id,
                    label: `${new Date(startedAt).toLocaleString()} · ${templateName}`
                  }))}
                  value={historyId}
                />
              </>
            ) : (
              <div className="server-command-batch-message" role="status">
                {history && <AppIcon name="history" size={18} />}
                <p>{history ? t.serverBatchNoHistory : t.serverBatchLoading}</p>
              </div>
            ))}
          {selectedRun && (
            <>
              {selectedRun.historyError && (
                <p role="alert">
                  {t.serverBatchHistoryError} {selectedRun.historyError}
                </p>
              )}
              <div className="server-command-batch-results">
                <table aria-label={t.serverBatchResults}>
                  <thead>
                    <tr>
                      <th scope="col">{t.host}</th>
                      <th scope="col">{t.serverDashboardColumnStatus}</th>
                      <th scope="col">{t.serverBatchExitCode}</th>
                    </tr>
                  </thead>
                  {selectedRun.results.map((result) => {
                    const target =
                      mode.type === 'run'
                        ? mode.targets.find(({ profileId }) => profileId === result.profileId)
                        : undefined
                    const output = result.error ?? result.output
                    return (
                      <tbody key={result.profileId}>
                        <tr className="server-command-batch-result">
                          <th scope="row">
                            {target?.name ?? result.name}
                            {target && <small>{target.address}</small>}
                          </th>
                          <td>
                            <span className="server-command-batch-status" data-status={result.status}>
                              {statuses[result.status]}
                            </span>
                          </td>
                          <td className="server-command-batch-exit-code">{result.exitCode ?? '—'}</td>
                        </tr>
                        {(output || result.outputTruncated) && (
                          <tr className="server-command-batch-output">
                            <td colSpan={3}>
                              {output && (
                                <details>
                                  <summary>
                                    <AppIcon name="chevron-right" size={12} />
                                    {result.error ? t.serverBatchError : t.serverBatchOutput}
                                  </summary>
                                  <pre className="server-command-batch-text">{output}</pre>
                                </details>
                              )}
                              {result.outputTruncated && <small>{t.serverBatchOutputTruncated}</small>}
                            </td>
                          </tr>
                        )}
                      </tbody>
                    )
                  })}
                </table>
              </div>
              {browsing && <p className="server-command-batch-history-note">{t.serverBatchHistoryMetadata}</p>}
            </>
          )}
        </div>
      </FormDialogBody>
    </Dialog>,
    document.body
  )
}
