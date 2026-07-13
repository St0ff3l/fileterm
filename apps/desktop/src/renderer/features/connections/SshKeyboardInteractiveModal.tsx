import { useEffect, useState } from 'react'
import type { SshKeyboardInteractiveRequest } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { t } from '../../i18n'

export function SshKeyboardInteractiveModal({
  request,
  errorMessage,
  onCancel,
  onSubmit
}: {
  request: SshKeyboardInteractiveRequest
  errorMessage?: string | null
  onCancel(): void
  onSubmit(answers: string[]): void
}) {
  const [answers, setAnswers] = useState<string[]>(() => request.prompts.map(() => ''))
  useEffect(() => setAnswers(request.prompts.map(() => '')), [request])
  const canSubmit =
    answers.length === request.prompts.length &&
    answers.every((answer, index) => request.prompts[index]?.echo || answer.length > 0)
  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>SSH keyboard-interactive</span>
          <CloseButton onClick={onCancel} />
        </div>
        {request.instructions ? <div className="root-access-description">{request.instructions}</div> : null}
        <div className="root-access-meta">
          <span>{t.host}</span>
          <strong>{`${request.host}:${request.port}`}</strong>
        </div>
        {request.prompts.map((prompt, index) => (
          <label className="file-action-field" key={`${index}-${prompt.prompt}`}>
            <span>{prompt.prompt || `Challenge ${index + 1}`}</span>
            <input
              autoFocus={index === 0}
              type={prompt.echo ? 'text' : 'password'}
              value={answers[index] ?? ''}
              onChange={(event) =>
                setAnswers((current) =>
                  current.map((value, itemIndex) => (itemIndex === index ? event.target.value : value))
                )
              }
              onKeyDown={(event) => {
                if (event.key === 'Enter' && canSubmit) onSubmit(answers)
              }}
            />
          </label>
        ))}
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="primary-button" disabled={!canSubmit} onClick={() => onSubmit(answers)} type="button">
            {t.sshAuthPromptConfirm}
          </button>
        </div>
      </div>
    </div>
  )
}
