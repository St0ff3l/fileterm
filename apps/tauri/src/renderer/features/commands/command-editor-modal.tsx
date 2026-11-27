import './command-editor-modal.css'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@fileterm/core'
import { t } from '../../i18n'
import { extractCommandParams, sortByOrder } from './command-utils'
import { CommandCodeEditor } from './command-code-editor'
import { AppIcon } from '../common/app-icon'
import { CloseButton } from '../common/close-button'
import { DropdownSelect } from '../common/dropdown-select'
import { SelectionControl } from '../common/selection-control'
import { StableButtonContent } from '../common/stable-button-content'

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
  children,
  footer
}: {
  title: string
  isSubmitting: boolean
  onClose(): void
  children: ReactNode
  footer: ReactNode
}) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const dialog = (
    <div className="modal-card command-dialog command-editor-page" onClick={(event) => event.stopPropagation()}>
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <MaterialIcon name="terminal" className="material-symbols-outlined" />
          <span>{title}</span>
        </span>
        <div className="connection-manager-header-actions">
          <CloseButton disabled={isSubmitting} onClick={onClose} />
        </div>
      </div>
      <div className="command-dialog-scroll-region">
        <div className="command-dialog-body scrollbar-scroll" ref={scrollRef}>
          {children}
        </div>
        <VerticalScrollbar scrollRef={scrollRef} />
      </div>
      {footer}
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
  const codeTextareaRef = useRef<HTMLTextAreaElement>(null)
  const submitLocked = externalIsSubmitting || isSubmitting
  const orderedFolders = useMemo(() => sortByOrder(folders), [folders])
  const detectedParams = useMemo(() => extractCommandParams(form.command), [form.command])
  const appendCarriageReturn = form.appendCarriageReturn ?? true
  const commandInvalid = showValidation && !form.command?.trim()
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

  const insertPlaceholder = (index: number) => {
    const token = `[p#${index}]`
    const textarea = codeTextareaRef.current
    if (!textarea) {
      setForm((prev) => ({ ...prev, command: `${prev.command}${token}` }))
      return
    }
    const start = textarea.selectionStart ?? form.command.length
    const end = textarea.selectionEnd ?? start
    setForm((prev) => ({
      ...prev,
      command: `${prev.command.slice(0, start)}${token}${prev.command.slice(end)}`
    }))
    requestAnimationFrame(() => {
      const nextCaret = start + token.length
      textarea.focus()
      textarea.setSelectionRange(nextCaret, nextCaret)
    })
  }

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
    <fieldset disabled={submitLocked} className="command-editor-fieldset">
      <div className="command-editor-stack">
        <div className="command-editor-meta-section">
          <div className="command-editor-row">
            <label className="command-editor-field">
              <span className="command-editor-field__label">{t.name}</span>
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
              <span className="command-editor-field__label">{t.commandCategory}</span>
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
          <label className="command-editor-field">
            <span className="command-editor-field__label">{t.note}</span>
            <input
              type="text"
              value={form.description ?? ''}
              onChange={(event) => {
                const { value } = event.target
                setForm((prev) => ({ ...prev, description: value }))
              }}
            />
          </label>
        </div>

        <div className={`command-editor-workbench ${commandInvalid ? 'is-invalid' : ''}`.trim()}>
          <div className="command-editor-workbench__toolbar">
            <span className="command-editor-workbench__heading">
              <AppIcon name="terminal" size={13} />
              <span>{t.commandTemplate}</span>
            </span>
            <div className="command-editor-workbench__params" role="toolbar" aria-label={t.commandParamHint}>
              <span className="command-editor-workbench__params-label">{t.commandParamHint}</span>
              <div className="command-editor-workbench__chips">
                {[1, 2, 3, 4, 5].map((index) => (
                  <button
                    key={index}
                    aria-label={`${t.commandParam}${index}`}
                    className="command-editor-param-chip"
                    title={`${t.commandParam}${index} ([p#${index}])`}
                    type="button"
                    onClick={() => insertPlaceholder(index)}
                  >
                    {`p#${index}`}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="command-editor-workbench__editor">
            <CommandCodeEditor
              value={form.command}
              onChange={(value) => setForm((prev) => ({ ...prev, command: value }))}
              ariaLabel={t.commandTemplate}
              className={commandInvalid ? 'is-invalid' : ''}
              minLines={standalone ? 12 : 8}
              textareaRef={codeTextareaRef}
            />
          </div>

          <div className="command-editor-workbench__statusbar">
            <div className="command-editor-workbench__status-left">
              {detectedParams.length > 0 ? (
                <>
                  <span className="command-editor-detected-params__label">{t.commandDetectedParams}</span>
                  <span className="command-editor-detected-params__list">
                    {detectedParams.map((param) => (
                      <code key={param} className="command-editor-detected-params__tag">
                        {param}
                      </code>
                    ))}
                  </span>
                </>
              ) : (
                <span className="command-editor-workbench__tip">
                  <AppIcon name="info" size={12} />
                  <span>{t.commandParamExplain}</span>
                </span>
              )}
            </div>
            <span
              aria-hidden="true"
              className={`command-editor-cr-badge ${appendCarriageReturn ? 'is-active' : ''}`.trim()}
            >
              ↵ CR
            </span>
          </div>
        </div>
      </div>
    </fieldset>
  )

  const footerActions = (
    <div className="form-actions command-dialog-actions command-editor-footer">
      <label className="command-editor-checkbox-row">
        <SelectionControl
          checked={appendCarriageReturn}
          disabled={submitLocked}
          type="checkbox"
          onChange={(event) => {
            const { checked } = event.target
            setForm((prev) => ({ ...prev, appendCarriageReturn: checked }))
          }}
        />
        <span>{t.commandAppendCr}</span>
      </label>
      <div className="command-editor-footer__buttons">
        <button className="flat-button" disabled={submitLocked} type="button" onClick={onClose}>
          {t.cancel}
        </button>
        <button
          aria-busy={submitLocked}
          className="primary-button"
          disabled={submitLocked}
          type="button"
          onClick={() => void submitForm()}
        >
          <StableButtonContent busy={submitLocked} label={t.save} />
        </button>
      </div>
    </div>
  )

  if (standalone) {
    return (
      <div className="connection-form-window">
        <div className="modal-card command-form-standalone">
          <div className="connection-manager-header" data-tauri-drag-region="deep">
            <span className="connection-manager-title">
              <MaterialIcon name="terminal" className="material-symbols-outlined" />
              <span>{title}</span>
            </span>
            <div className="connection-manager-header-actions">
              <CloseButton disabled={submitLocked} onClick={onClose} />
            </div>
          </div>
          <div className="command-form-standalone-body">
            <div className="command-form-standalone-page scrollbar-scroll">
              {editorFields}
              {submitError ? <div className="modal-error">{submitError}</div> : null}
            </div>
            {footerActions}
          </div>
        </div>
      </div>
    )
  }

  return (
    <CommandDialogShell title={title} isSubmitting={submitLocked} onClose={onClose} footer={footerActions}>
      <div className="command-editor-dialog-form">
        {editorFields}
        {submitError ? <div className="modal-error">{submitError}</div> : null}
      </div>
    </CommandDialogShell>
  )
}
