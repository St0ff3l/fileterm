import { useEffect, useState } from 'react'
import type { SshKeyboardInteractiveRequest } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { StableButtonContent } from '../common/StableButtonContent'
import { t } from '../../i18n'

export function SshKeyboardInteractiveModal({
  request,
  errorMessage,
  isSubmitting = false,
  onCancel,
  onSubmit
}: {
  request: SshKeyboardInteractiveRequest
  errorMessage?: string | null
  isSubmitting?: boolean
  onCancel(): void
  onSubmit(answers: string[]): void
}) {
  const [answers, setAnswers] = useState<string[]>(() => request.prompts.map(() => ''))
  useEffect(() => setAnswers(request.prompts.map(() => '')), [request])
  const authenticationTarget =
    request.authenticationTarget === 'jump-host'
      ? t.sshAuthenticationTargetJumpHost
      : request.authenticationTarget === 'target'
        ? t.sshAuthenticationTargetTarget
        : t.sshAuthenticationTargetDirect
  const canSubmit =
    answers.length === request.prompts.length &&
    answers.every((answer, index) => request.prompts[index]?.echo || answer.length > 0)
  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>{t.sshKeyboardInteractiveTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>
        {request.name ? <div className="root-access-description">{request.name}</div> : null}
        {request.instructions ? <div className="root-access-description">{request.instructions}</div> : null}
        <div className="root-access-meta">
          <span>{t.sshKeyboardInteractiveConnection}</span>
          <strong>{`${authenticationTarget} · ${request.connectionName}`}</strong>
        </div>
        <div className="root-access-meta">
          <span>{t.host}</span>
          <strong>{`${request.host}:${request.port}`}</strong>
        </div>
        <div className="root-access-meta">
          <span>{t.sshKeyboardInteractiveRound.replace('{round}', String(request.round))}</span>
        </div>
        {request.prompts.map((prompt, index) => (
          <label className="file-action-field" key={`${index}-${prompt.prompt}`}>
            <span>{prompt.prompt || `Challenge ${index + 1}`}</span>
            <input
              autoFocus={index === 0}
              disabled={isSubmitting}
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
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button"
            disabled={!canSubmit || isSubmitting}
            onClick={() => onSubmit(answers)}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.sshAuthPromptConfirm} />
          </button>
        </div>
      </div>
    </div>
  )
}
