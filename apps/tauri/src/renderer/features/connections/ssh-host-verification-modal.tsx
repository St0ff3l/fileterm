import type { SshHostVerificationRequest } from '@fileterm/core'
import { CloseButton } from '../common/close-button'
import { StableButtonContent } from '../common/stable-button-content'
import { t } from '../../i18n'
import { sshInteractionConnectionLabel } from './ssh-interaction-labels'

export function SshHostVerificationModal({
  isSubmitting = false,
  request,
  onAcceptAndSave,
  onAcceptOnce,
  onReject
}: {
  isSubmitting?: boolean
  request: SshHostVerificationRequest
  onAcceptAndSave(): void
  onAcceptOnce(): void
  onReject(): void
}) {
  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal ssh-host-verification-modal">
        <div className="modal-header">
          <span>{t.sshHostVerificationTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onReject} />
        </div>

        <div className="ssh-interaction-modal__body">
          <fieldset className="ssh-fieldset ssh-verification-fieldset">
            <legend>{t.general}</legend>
            <div className="root-access-description">{t.sshHostVerificationDescription}</div>

            <div className="root-access-meta">
              <span>{t.sshKeyboardInteractiveConnection}</span>
              <strong>{sshInteractionConnectionLabel(request.authenticationTarget, request.connectionName)}</strong>
            </div>

            <div className="root-access-meta">
              <span>{t.host}</span>
              <strong>{`${request.host}:${request.port}`}</strong>
            </div>
          </fieldset>

          {request.knownFingerprint ? <div className="modal-error">{t.sshHostVerificationChanged}</div> : null}

          <fieldset className="ssh-fieldset ssh-verification-fieldset">
            <legend>{t.sshHostFingerprintLabel}</legend>
            <div className="ssh-verification-box">
              <span>{t.sshHostFingerprintLabel}</span>
              <strong>{request.fingerprint}</strong>
            </div>

            {request.knownFingerprint ? (
              <div className="ssh-verification-box">
                <span>{t.sshHostKnownFingerprintLabel}</span>
                <strong>{request.knownFingerprint}</strong>
              </div>
            ) : null}
          </fieldset>
        </div>

        <div className="form-actions ssh-verification-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onReject} type="button">
            {t.sshHostReject}
          </button>
          <button className="flat-button" disabled={isSubmitting} onClick={onAcceptOnce} type="button">
            {t.sshHostAcceptOnce}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button"
            disabled={isSubmitting}
            onClick={onAcceptAndSave}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.sshHostAcceptAndSave} />
          </button>
        </div>
      </div>
    </div>
  )
}
