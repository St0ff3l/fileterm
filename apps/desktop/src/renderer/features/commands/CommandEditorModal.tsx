import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@fileterm/core'
import { t } from '../../i18n'
import { extractCommandParams, sortByOrder } from './command-utils'
import { CloseButton } from '../common/CloseButton'

export const emptyCommandForm: CommandTemplateInput = {
  name: '',
  command: '',
  description: '',
  parentId: undefined,
  appendCarriageReturn: true
}

const COMMAND_EDITOR_MIN_LINE_COUNT = 14

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

function CommandDialogShell({ title, onClose, children }: { title: string; onClose(): void; children: ReactNode }) {
  const dialog = (
    <div className="modal-card command-dialog command-editor-page" onClick={(event) => event.stopPropagation()}>
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">terminal</span>
          <span>{title}</span>
        </span>
        <div className="connection-manager-header-actions">
          <CloseButton onClick={onClose} />
        </div>
      </div>
      <div className="command-dialog-body scrollbar-scroll">{children}</div>
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

  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const lineNumRef = useRef<HTMLDivElement>(null)

  const handleTextareaScroll = () => {
    if (lineNumRef.current && textareaRef.current) {
      lineNumRef.current.scrollTop = textareaRef.current.scrollTop
    }
  }

  const editorFields = (
    <div className="command-editor-grid">
      <div className="command-editor-row full">
        <label className="command-editor-field">
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
          <span className="ft-select-shell command-select-shell">
            <select
              value={form.parentId ?? ''}
              onChange={(event) => {
                const { value } = event.target
                setForm((prev) => ({ ...prev, parentId: value || undefined }))
              }}
            >
              <option value="">{t.commandUncategorized}</option>
              {orderedFolders.map((folder) => (
                <option key={folder.id} value={folder.id}>
                  {folder.name}
                </option>
              ))}
            </select>
            <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
              expand_more
            </span>
          </span>
        </label>
      </div>
      <label className="command-editor-field full">
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
      <div className="command-editor-field full command-editor-dialog-textarea">
        <span>{t.commandTemplate}</span>
        <div className="command-code-area">
          <div className="command-line-numbers" ref={lineNumRef} aria-hidden="true">
            {Array.from(
              { length: Math.max(form.command.split('\n').length, COMMAND_EDITOR_MIN_LINE_COUNT) },
              (_, i) => (
                <div key={i} className="command-line-number">
                  {i + 1}
                </div>
              )
            )}
          </div>
          <textarea
            ref={textareaRef}
            rows={12}
            value={form.command}
            spellCheck={false}
            className={showValidation && !form.command?.trim() ? 'is-invalid' : ''}
            onChange={(event) => {
              const { value } = event.target
              setForm((prev) => ({ ...prev, command: value }))
            }}
            onScroll={handleTextareaScroll}
          />
        </div>
      </div>
      <div className="command-editor-field full command-editor-dialog-params">
        <span>{t.commandParamHint}</span>
        <div className="command-param-hints-row">
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
        <code className={extractCommandParams(form.command).length === 0 ? 'is-empty' : ''}>
          {extractCommandParams(form.command).join(', ') || '-'}
        </code>
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
              <CloseButton onClick={onClose} />
            </div>
          </div>
          <div className="command-form-standalone-body">
            <div className="command-form-standalone-page scrollbar-scroll">{editorFields}</div>
            <div className="form-actions command-dialog-actions">
              <button className="flat-button" type="button" onClick={onClose}>
                {t.cancel}
              </button>
              <button className="primary-button" type="button" onClick={submitForm}>
                {t.save}
              </button>
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
          <button className="flat-button" type="button" onClick={onClose}>
            {t.cancel}
          </button>
          <button className="primary-button" type="button" onClick={submitForm}>
            {t.save}
          </button>
        </div>
      </div>
    </CommandDialogShell>
  )
}
