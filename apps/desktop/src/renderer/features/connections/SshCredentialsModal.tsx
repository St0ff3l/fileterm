import { useEffect, useState } from 'react'
import type { SshCredentialsPromptRequest } from '@termdock/core'
import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

export function SshCredentialsModal({
  errorMessage,
  request,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  request: SshCredentialsPromptRequest
  onCancel(): void
  onSubmit(input: { username: string; password: string }): void
}) {
  const [username, setUsername] = useState(request.username ?? '')
  const [password, setPassword] = useState('')

  useEffect(() => {
    setUsername(request.username ?? '')
    setPassword('')
  }, [request])

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>{t.sshAuthPromptTitle}</span>
          <button className="icon-button" onClick={onCancel} type="button">
            <AppIcon name="close" size={16} />
          </button>
        </div>

        <div className="root-access-description">{t.sshAuthPromptDescription}</div>

        <div className="root-access-meta">
          <span>{t.host}</span>
          <strong>{`${request.host}:${request.port}`}</strong>
        </div>

        <label className="file-action-field">
          <span>{t.sshAuthPromptUsername}</span>
          <input
            autoFocus
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                onSubmit({ username, password })
              }
            }}
          />
        </label>

        <label className="file-action-field">
          <span>{t.password}</span>
          <input
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                onSubmit({ username, password })
              }
            }}
          />
        </label>

        {request.passwordRequired ? <div className="file-action-hint">{t.sshAuthPromptPasswordRequired}</div> : null}
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

        <div className="form-actions">
          <button className="flat-button" onClick={onCancel} type="button">{t.cancel}</button>
          <button className="primary-button" onClick={() => onSubmit({ username, password })} type="button">
            {t.sshAuthPromptConfirm}
          </button>
        </div>
      </div>
    </div>
  )
}
