import type { SshHostVerificationRequest } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { StableButtonContent } from '../common/StableButtonContent'
import { t } from '../../i18n'

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
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>{t.sshHostVerificationTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onReject} />
        </div>

        <div className="root-access-description">{t.sshHostVerificationDescription}</div>
        {request.knownFingerprint ? <div className="modal-error">{t.sshHostVerificationChanged}</div> : null}

        <div className="root-access-meta">
          <span>{t.host}</span>
          <strong>{`${request.host}:${request.port}`}</strong>
        </div>

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
