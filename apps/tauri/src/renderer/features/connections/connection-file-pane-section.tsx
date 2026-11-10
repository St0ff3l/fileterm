import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionFilePaneSection({
  form,
  setForm
}: {
  form: CreateProfileInput
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset narrow">
        <legend>{t.filePaneDefaultMode}</legend>
        <div className="ssh-grid single">
          <div className="span-2 ssh-form-field">
            <div className="ssh-form-field__header">
              <label htmlFor="connection-file-pane-mode">{t.filePaneDisplayMode}</label>
            </div>
            <DropdownSelect
              id="connection-file-pane-mode"
              ariaLabel={t.filePaneDefaultMode}
              options={[
                { value: 'local', label: t.filePaneLocal },
                { value: 'remote-tree', label: t.remoteDirectoryTree }
              ]}
              value={form.filePaneMode ?? 'local'}
              onChange={(value) =>
                setForm((previous) => ({
                  ...previous,
                  filePaneMode: value === 'remote-tree' ? 'remote-tree' : 'local'
                }))
              }
            />
            <p className="ssh-field-hint">{t.filePaneDefaultModeHint}</p>
          </div>
        </div>
      </fieldset>
      <fieldset className="ssh-fieldset narrow">
        <legend>{t.characterEncoding}</legend>
        <div className="ssh-grid single">
          <div className="span-2 ssh-form-field">
            <DropdownSelect
              ariaLabel={t.characterEncoding}
              id="connection-file-encoding"
              value={form.encoding ?? 'UTF-8'}
              options={[
                { value: 'UTF-8', label: 'UTF-8' },
                { value: 'GBK', label: 'GBK' }
              ]}
              onChange={(value) => setForm((previous) => ({ ...previous, encoding: value }))}
            />
            {form.type === 'ssh' || form.type === 'sftp' ? (
              <p className="ssh-field-hint">{t.sftpFilenameEncodingHint}</p>
            ) : null}
          </div>
        </div>
      </fieldset>
    </div>
  )
}
