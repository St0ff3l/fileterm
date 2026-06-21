import type { SshHostVerificationRequest } from '@termdock/core'
import { AppIcon } from '../common/AppIcon'
import { t } from '../../i18n'

export function SshHostVerificationModal({
  request,
  onAcceptAndSave,
  onAcceptOnce,
  onReject
}: {
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
          <button className="icon-button" onClick={onReject} type="button">
            <AppIcon name="close" size={16} />
          </button>
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
          <button className="flat-button" onClick={onReject} type="button">{t.sshHostReject}</button>
          <button className="flat-button" onClick={onAcceptOnce} type="button">{t.sshHostAcceptOnce}</button>
          <button className="primary-button" onClick={onAcceptAndSave} type="button">{t.sshHostAcceptAndSave}</button>
        </div>
      </div>
    </div>
  )
}
