import type { CreateProfileInput } from '@fileterm/core'
import { t } from '../../i18n'
import type { ConnectionFormSetter } from './connection-modal-utils'
import { ConnectionNoDeleteSetting } from './connection-no-delete-setting'
import { TransferSidebarSettingControl } from '../../components/common/transfer-sidebar-setting-control'

export function ConnectionTransferAdvancedSection({
  defaultTransferSidebarCollapsedByDefault,
  form,
  setForm
}: {
  defaultTransferSidebarCollapsedByDefault: boolean
  form: CreateProfileInput
  setForm: ConnectionFormSetter
}) {
  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset">
        <legend>{t.advancedSettings}</legend>
        <div className="advanced-toggle-list">
          <TransferSidebarSettingControl
            checked={form.transferSidebarCollapsedByDefault ?? defaultTransferSidebarCollapsedByDefault}
            hint={t.connectionTransferSidebarCollapsedByDefaultHint}
            label={t.connectionTransferSidebarCollapsedByDefault}
            onChange={(value) => setForm((previous) => ({ ...previous, transferSidebarCollapsedByDefault: value }))}
          />
          {form.type === 'sftp' ? <ConnectionNoDeleteSetting form={form} setForm={setForm} /> : null}
        </div>
      </fieldset>
    </div>
  )
}
