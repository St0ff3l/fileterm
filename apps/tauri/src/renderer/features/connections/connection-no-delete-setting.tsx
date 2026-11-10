import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { SelectionControl } from '../common/selection-control'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionNoDeleteSetting({
  form,
  setForm
}: {
  form: CreateProfileInput
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="advanced-toggle-row">
      <label className="ssh-checkbox advanced-toggle-label">
        <SelectionControl
          type="checkbox"
          checked={form.sftpNoDelete ?? false}
          onChange={(event) => setForm((previous) => ({ ...previous, sftpNoDelete: event.target.checked }))}
        />
        <span className="advanced-toggle-name">{t.sftpNoDelete}</span>
      </label>
      <p className="advanced-toggle-hint">{t.sftpNoDeleteHint}</p>
    </div>
  )
}
