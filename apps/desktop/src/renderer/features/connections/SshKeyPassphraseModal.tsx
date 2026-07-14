import { useEffect, useState } from 'react'
import type { SshKeyPassphrasePromptRequest } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'

export function SshKeyPassphraseModal({
  errorMessage,
  request,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  request: SshKeyPassphrasePromptRequest
  onCancel(): void
  onSubmit(input: { passphrase: string; savePassphrase: boolean }): void
}) {
  const [passphrase, setPassphrase] = useState('')
  const [savePassphrase, setSavePassphrase] = useState(false)

  useEffect(() => {
    setPassphrase('')
    setSavePassphrase(false)
  }, [request])

  const submit = () => onSubmit({ passphrase, savePassphrase })

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>SSH 私钥口令</span>
          <CloseButton onClick={onCancel} />
        </div>
        <div className="root-access-description">
          {request.reason === 'invalid-saved'
            ? '已保存的口令无法解锁该私钥，请重新输入。'
            : '该私钥已加密，请输入口令。'}
        </div>
        <div className="root-access-meta">
          <span>密钥</span>
          <strong>{request.keyName}</strong>
        </div>
        <label className="file-action-field">
          <span>口令</span>
          <input
            autoFocus
            type="password"
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') submit()
            }}
          />
        </label>
        <label className="ssh-checkbox">
          <input
            checked={savePassphrase}
            type="checkbox"
            onChange={(event) => setSavePassphrase(event.target.checked)}
          />
          <span>保存此密钥口令，后续自动使用</span>
        </label>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" onClick={onCancel} type="button">
            取消
          </button>
          <button className="primary-button" disabled={!passphrase} onClick={submit} type="button">
            继续连接
          </button>
        </div>
      </div>
    </div>
  )
}
