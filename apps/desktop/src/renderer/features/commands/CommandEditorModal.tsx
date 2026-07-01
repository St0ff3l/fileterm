import { useEffect, useMemo, useState, type ClipboardEvent, type ReactNode } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@fileterm/core'
import { t } from '../../i18n'
import { extractCommandParams, sortByOrder } from './command-utils'
import { AppIcon } from '../common/AppIcon'

export const emptyCommandForm: CommandTemplateInput = {
  name: '',
  command: '',
  description: '',
  parentId: undefined,
  appendCarriageReturn: true
}

export function toCommandTemplateInput(command: CommandTemplate): CommandTemplateInput {
  return {
    name: command.name,
    command: command.command,
    description: command.description ?? '',
    parentId: command.parentId,
    order: command.order,
    appendCarriageReturn: command.appendCarriageReturn
  }
}

function CommandDialogShell({
  title,
  onClose,
  children
}: {
  title: string
  onClose(): void
  children: ReactNode
}) {
  const dialog = (
    <div className="modal-card command-dialog command-editor-page" onClick={(event) => event.stopPropagation()}>
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">terminal</span>
          <span>{title}</span>
        </span>
        <div className="connection-manager-header-actions">
          <button aria-label={t.closeTab} className="manager-close-button" onClick={onClose} title={t.closeTab} type="button">×</button>
        </div>
      </div>
      <div className="command-dialog-body scrollbar-scroll">
        {children}
      </div>
    </div>
  )

  return (
    <div className="modal-backdrop command-dialog-backdrop" onClick={onClose}>
      {dialog}
    </div>
  )
}

export function CommandEditorModal({
  folders,
  initialValue,
  mode,
  standalone = false,
  onClose,
  onSubmit
}: {
  folders: CommandFolder[]
  initialValue: CommandTemplateInput
  mode: 'create' | 'edit'
  standalone?: boolean
  onClose(): void
  onSubmit(input: CommandTemplateInput): void
}) {
  const [form, setForm] = useState<CommandTemplateInput>(initialValue)
  const [showValidation, setShowValidation] = useState(false)
  const orderedFolders = useMemo(() => sortByOrder(folders), [folders])
  const title = mode === 'edit' && initialValue.name ? `${t.commandEdit}-${initialValue.name}` : t.commandCreate

  useEffect(() => {
    setForm(initialValue)
    setShowValidation(false)
  }, [
    mode,
    initialValue.name,
    initialValue.command,
    initialValue.description,
    initialValue.parentId,
    initialValue.order,
    initialValue.appendCarriageReturn
  ])

  const submitForm = () => {
    if (!form.name?.trim() || !form.command?.trim()) {
      setShowValidation(true)
      return
    }
    onSubmit({
      ...form,
      name: form.name.trim(),
      command: form.command.trim(),
      description: form.description?.trim() || undefined
    })
  }

  const editorFields = (
    <div className="command-editor-grid">
      <label className="command-editor-field full">
        <span>{t.name}</span>
        <input
          type="text"
          value={form.name}
          className={showValidation && !form.name?.trim() ? 'is-invalid' : ''}
          onChange={(event) => {
            const { value } = event.target
            setForm((prev) => ({ ...prev, name: value }))
          }}
        />
      </label>
      <label className="command-editor-field">
        <span>{t.commandCategory}</span>
        <select
          value={form.parentId ?? ''}
          onChange={(event) => {
            const { value } = event.target
            setForm((prev) => ({ ...prev, parentId: value || undefined }))
          }}
        >
          <option value="">{t.commandUncategorized}</option>
          {orderedFolders.map((folder) => (
            <option key={folder.id} value={folder.id}>{folder.name}</option>
          ))}
        </select>
      </label>
      <label className="command-editor-field">
        <span>{t.note}</span>
        <input
          type="text"
          value={form.description ?? ''}
          onChange={(event) => {
            const { value } = event.target
            setForm((prev) => ({ ...prev, description: value }))
          }}
        />
      </label>
      <label className="command-editor-field full command-editor-dialog-textarea">
        <span>{t.commandTemplate}</span>
        <textarea
          rows={12}
          value={form.command}
          className={showValidation && !form.command?.trim() ? 'is-invalid' : ''}
          onChange={(event) => {
            const { value } = event.target
            setForm((prev) => ({ ...prev, command: value }))
          }}
        />
      </label>
      <div className="command-editor-field full command-editor-dialog-params">
        <span>{t.commandParamHint}</span>
        <div className="command-param-hints">
          {[1, 2, 3, 4, 5].map((index) => (
            <button
              key={index}
              type="button"
              onClick={() => setForm((prev) => ({ ...prev, command: `${prev.command}[p#${index}]` }))}
            >
              {`${t.commandParam}${index}`}
            </button>
          ))}
        </div>
        <small>{t.commandParamExplain}</small>
      </div>
      <label className="command-editor-field full command-editor-checkbox-row">
        <input
          checked={form.appendCarriageReturn ?? true}
          type="checkbox"
          onChange={(event) => {
            const { checked } = event.target
            setForm((prev) => ({ ...prev, appendCarriageReturn: checked }))
          }}
        />
        <span>{t.commandAppendCr}</span>
      </label>
      <div className="command-editor-field full command-preview">
        <span>{t.commandDetectedParams}</span>
        <code>{extractCommandParams(form.command).join(', ') || '-'}</code>
      </div>
    </div>
  )

  if (standalone) {
    return (
      <div className="connection-form-window">
        <div className="modal-card command-form-standalone">
          <div className="connection-manager-header">
            <span className="connection-manager-title">
              <span className="material-symbols-outlined">terminal</span>
              <span>{title}</span>
            </span>
            <div className="connection-manager-header-actions">
              <button aria-label={t.closeTab} className="manager-close-button" onClick={onClose} title={t.closeTab} type="button">×</button>
            </div>
          </div>
          <div className="command-form-standalone-body">
            <div className="command-form-standalone-page scrollbar-scroll">
              {editorFields}
            </div>
            <div className="form-actions command-dialog-actions">
              <button className="flat-button" type="button" onClick={onClose}>{t.cancel}</button>
              <button className="primary-button" type="button" onClick={submitForm}>{t.save}</button>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <CommandDialogShell title={title} onClose={onClose}>
      <div className="command-editor-dialog-form">
        {editorFields}
        <div className="form-actions command-dialog-actions">
          <button className="flat-button" type="button" onClick={onClose}>{t.cancel}</button>
          <button className="primary-button" type="button" onClick={submitForm}>{t.save}</button>
        </div>
      </div>
    </CommandDialogShell>
  )
}
