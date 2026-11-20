import type { CommandBatchRun, CommandBatchTarget, CommandTemplate } from '@fileterm/core'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Button } from '../../components/common/button/button'
import { Dialog } from '../../components/common/dialog/dialog'
import { FormDialogBody } from '../../components/common/dialog/form-dialog-body'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../components/common/input/input'
import { formatMessage, t } from '../../i18n'
import { extractCommandParams, sortByOrder } from '../commands/command-utils'
import './server-command-batch-dialog.css'

export type ServerCommandBatchDialogMode =
  { type: 'run'; targets: Array<CommandBatchTarget & { label: string }> } | { type: 'history' }

export function ServerCommandBatchDialog({
  mode,
  templates,
  onClose
}: {
  mode: ServerCommandBatchDialogMode
  templates: CommandTemplate[]
  onClose(): void
}) {
  const desktopApi = window.fileterm
  const [templateId, setTemplateId] = useState(() => sortByOrder(templates)[0]?.id ?? '')
  const [args, setArgs] = useState<Record<number, string>>({})
  const [run, setRun] = useState<CommandBatchRun | null>(null)
  const [history, setHistory] = useState<CommandBatchRun[] | null>(null)
  const [historyId, setHistoryId] = useState('')
  const [running, setRunning] = useState(false)
  const [error, setError] = useState('')
  const submitting = useRef(false)
  const browsing = mode.type === 'history'
  const selectedRun = browsing ? history?.find(({ id }) => id === historyId) : run
  const template = templates.find(({ id }) => id === templateId)
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
        <>
          <Button disabled={running} onClick={close} size="panel">
            {browsing || run ? t.serverBatchClose : t.cancel}
          </Button>
          {!browsing && !run && (
            <Button disabled={!canRun} loading={running} onClick={() => void start()} size="panel" variant="primary">
              {t.serverBatchRun}
            </Button>
          )}
        </>
      }
      isOpen
      onClose={close}
      showCloseButton={!running}
      size="lg"
      title={title}
    >
      <FormDialogBody>
        {error && <p role="alert">{error}</p>}
        {running && <p role="status">{t.serverBatchRunning}</p>}
        {mode.type === 'run' && !run && (
          <>
            <label htmlFor="server-batch-template">{t.serverBatchTemplate}</label>
            <DropdownSelect
              disabled={running}
              id="server-batch-template"
              onChange={(id) => {
                setArgs({})
                setTemplateId(id)
              }}
              options={sortByOrder(templates).map(({ id, name }) => ({ value: id, label: name }))}
              value={templateId}
            />
            {template && <pre className="server-command-batch-text">{template.command}</pre>}
            {params.map((index) => (
              <label key={index}>
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
            <div>
              <strong>{formatMessage(t.serverBatchTargets, { count: mode.targets.length })}</strong>
              <ul className="server-command-batch-targets">
                {mode.targets.map(({ profileId, label }) => (
                  <li key={profileId}>{label}</li>
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
                onChange={setHistoryId}
                options={history.map(({ id, startedAt, templateName }) => ({
                  value: id,
                  label: `${new Date(startedAt).toLocaleString()} · ${templateName}`
                }))}
                value={historyId}
              />
              <p>{t.serverBatchHistoryMetadata}</p>
            </>
          ) : (
            <p role="status">{history ? t.serverBatchNoHistory : t.serverBatchLoading}</p>
          ))}
        {selectedRun && (
          <>
            {selectedRun.historyError && (
              <p role="alert">
                {t.serverBatchHistoryError} {selectedRun.historyError}
              </p>
            )}
            <ul className="server-command-batch-results">
              {selectedRun.results.map((result) => (
                <li key={result.profileId}>
                  <div className="server-command-batch-result">
                    <strong>
                      {mode.type === 'run'
                        ? (mode.targets.find(({ profileId }) => profileId === result.profileId)?.label ?? result.name)
                        : result.name}
                    </strong>
                    <span>{statuses[result.status]}</span>
                    <span>{formatMessage(t.serverBatchExitCode, { code: result.exitCode ?? '—' })}</span>
                  </div>
                  {(result.output || result.error) && (
                    <details>
                      <summary>{result.error ? t.serverBatchError : t.serverBatchOutput}</summary>
                      <pre className="server-command-batch-text">{result.error ?? result.output}</pre>
                    </details>
                  )}
                  {result.outputTruncated && <small>{t.serverBatchOutputTruncated}</small>}
                </li>
              ))}
            </ul>
          </>
        )}
      </FormDialogBody>
    </Dialog>,
    document.body
  )
}
