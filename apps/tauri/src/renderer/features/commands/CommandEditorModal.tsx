import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@fileterm/core'
import { t } from '../../i18n'
import { extractCommandParams, sortByOrder } from './command-utils'
import { CommandCodeEditor } from './CommandCodeEditor'
import { CloseButton } from '../common/CloseButton'
import { DropdownSelect } from '../common/DropdownSelect'

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
  isSubmitting,
  onClose,
  children
}: {
  title: string
  isSubmitting: boolean
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
          <CloseButton disabled={isSubmitting} onClick={onClose} />
        </div>
      </div>
      <div className="command-dialog-body scrollbar-scroll">{children}</div>
    </div>
  )

  return (
    <div className="modal-backdrop command-dialog-backdrop" onClick={isSubmitting ? undefined : onClose}>
      {dialog}
    </div>
  )
}

export function CommandEditorModal({
  folders,
  initialValue,
  isSubmitting: externalIsSubmitting = false,
  mode,
  standalone = false,
  onClose,
  onSubmit
}: {
  folders: CommandFolder[]
  initialValue: CommandTemplateInput
  isSubmitting?: boolean
  mode: 'create' | 'edit'
  standalone?: boolean
  onClose(): void
  onSubmit(input: CommandTemplateInput): Promise<boolean | void> | boolean | void
}) {
  const [form, setForm] = useState<CommandTemplateInput>(initialValue)
  const [showValidation, setShowValidation] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)
  const submittingRef = useRef(false)
  const submitLocked = externalIsSubmitting || isSubmitting
  const orderedFolders = useMemo(() => sortByOrder(folders), [folders])
  const title = mode === 'edit' && initialValue.name ? `${t.commandEdit}-${initialValue.name}` : t.commandCreate

  useEffect(() => {
    setForm(initialValue)
    setShowValidation(false)
    setSubmitError(null)
  }, [
    mode,
    initialValue.name,
    initialValue.command,
    initialValue.description,
    initialValue.parentId,
    initialValue.order,
    initialValue.appendCarriageReturn
  ])

  const submitForm = async () => {
    if (submitLocked || submittingRef.current) {
      return
    }
    if (!form.name?.trim() || !form.command?.trim()) {
      setShowValidation(true)
      return
    }
    submittingRef.current = true
    setIsSubmitting(true)
    setSubmitError(null)
    try {
      await onSubmit({
        ...form,
        name: form.name.trim(),
        command: form.command.trim(),
        description: form.description?.trim() || undefined
      })
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : String(error))
    } finally {
      submittingRef.current = false
      setIsSubmitting(false)
    }
  }

  const editorFields = (
    <fieldset disabled={submitLocked} style={{ border: 0, display: 'contents', margin: 0, padding: 0 }}>
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
            <DropdownSelect
              className="command-select-shell"
              value={form.parentId ?? ''}
              options={[
                { value: '', label: t.commandUncategorized },
                ...orderedFolders.map((folder) => ({ value: folder.id, label: folder.name }))
              ]}
              onChange={(value) => setForm((prev) => ({ ...prev, parentId: value || undefined }))}
            />
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
          <CommandCodeEditor
            value={form.command}
            onChange={(value) => setForm((prev) => ({ ...prev, command: value }))}
            ariaLabel={t.commandTemplate}
            className={showValidation && !form.command?.trim() ? 'is-invalid' : ''}
          />
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
    </fieldset>
  )

  if (standalone) {
    return (
      <div className="connection-form-window">
        <div className="modal-card command-form-standalone">
          <div className="connection-manager-header" data-tauri-drag-region="deep">
            <span className="connection-manager-title">
              <span className="material-symbols-outlined">terminal</span>
              <span>{title}</span>
            </span>
            <div className="connection-manager-header-actions">
              <CloseButton disabled={submitLocked} onClick={onClose} />
            </div>
          </div>
          <div className="command-form-standalone-body">
            <div className="command-form-standalone-page scrollbar-scroll">{editorFields}</div>
            <div className="form-actions command-dialog-actions">
              <button className="flat-button" disabled={submitLocked} type="button" onClick={onClose}>
                {t.cancel}
              </button>
              <button
                className="primary-button"
                disabled={submitLocked}
                type="button"
                onClick={() => void submitForm()}
              >
                {submitLocked ? <span aria-hidden="true" className="button-spinner" /> : null}
                <span>{t.save}</span>
              </button>
            </div>
            {submitError ? <div className="modal-error">{submitError}</div> : null}
          </div>
        </div>
      </div>
    )
  }

  return (
    <CommandDialogShell title={title} isSubmitting={submitLocked} onClose={onClose}>
      <div className="command-editor-dialog-form">
        {editorFields}
        <div className="form-actions command-dialog-actions">
          <button className="flat-button" disabled={submitLocked} type="button" onClick={onClose}>
            {t.cancel}
          </button>
          <button className="primary-button" disabled={submitLocked} type="button" onClick={() => void submitForm()}>
            {submitLocked ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.save}</span>
          </button>
        </div>
        {submitError ? <div className="modal-error">{submitError}</div> : null}
      </div>
    </CommandDialogShell>
  )
}
