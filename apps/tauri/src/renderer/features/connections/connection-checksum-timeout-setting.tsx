import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import { SelectionControl } from '../common/selection-control'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionChecksumTimeoutSetting({
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
          checked={form.disableChecksumTimeoutCap === true}
          type="checkbox"
          onChange={(event) =>
            setForm((previous) => ({ ...previous, disableChecksumTimeoutCap: event.target.checked }))
          }
        />
        <span className="advanced-toggle-name">{t.disableChecksumTimeoutCap}</span>
      </label>
      <p className="advanced-toggle-hint">{t.disableChecksumTimeoutCapHint}</p>
    </div>
  )
}
