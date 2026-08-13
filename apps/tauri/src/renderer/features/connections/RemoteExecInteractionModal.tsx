import { useEffect, useState } from 'react'
import type { RemoteExecInteractionRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

/** A local-only credential / confirmation prompt for one isolated exec PTY. */
export function RemoteExecInteractionModal({
  request,
  errorMessage,
  isSubmitting = false,
  onCancel,
  onSubmit
}: {
  request: RemoteExecInteractionRequest
  errorMessage?: string | null
  isSubmitting?: boolean
  onCancel(): void
  onSubmit(value: string): void
}) {
  const [value, setValue] = useState('')
  useEffect(() => setValue(''), [request.requestId])

  const target = [request.shellUser, request.host].filter(Boolean).join('@') || request.host
  const submit = () => {
    if (value) onSubmit(value)
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal remote-exec-interaction-modal">
        <div className="modal-header">
          <span>{t.remoteExecInteractionTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>
        <p className="root-access-description">{t.remoteExecInteractionDescription}</p>
        <div className="root-access-meta">
          <span>{t.remoteExecInteractionTarget}</span>
          <strong>{target}</strong>
          {request.cwd ? <span>{request.cwd}</span> : null}
        </div>
        <label className="file-action-field">
          <span>{t.remoteExecInteractionCommand}</span>
          <input disabled readOnly value={request.command} />
        </label>
        <label className="file-action-field">
          <span>{request.prompt || t.remoteExecInteractionPromptFallback}</span>
          <input
            autoFocus
            disabled={isSubmitting}
            type={request.inputKind === 'secret' ? 'password' : 'text'}
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') submit()
            }}
          />
        </label>
        <p className="file-action-hint">{t.remoteExecInteractionPrivacy}</p>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="primary-button" disabled={!value || isSubmitting} onClick={submit} type="button">
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.remoteExecInteractionSend}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
