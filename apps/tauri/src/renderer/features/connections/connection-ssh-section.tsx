import type { ConnectionFormMode, CreateProfileInput, SerialPortInfo, SshConnectionDefaults } from '@fileterm/core'
import { ConnectionSshAuthSection } from './connection-ssh-auth-section'
import { ConnectionSshGeneralSection } from './connection-ssh-general-section'
import type { ConnectionFormSetter } from './connection-modal-utils'

export function ConnectionSshSection({
  connectionDefaults,
  form,
  groupOptions,
  hasSavedPassword,
  hasSavedSuPassword,
  hasSavedSudoPassword,
  isMacOs,
  isLoadingSerialPorts,
  isNetworkDevice,
  mode,
  onClearHostFingerprint,
  refreshSerialPorts,
  serialDevicePathPlaceholder,
  serialPortLoadError,
  serialPortOptions,
  serialPorts,
  setForm,
  showsNetworkDeviceVendor,
  supportsBuiltInRs485,
  supportsExtendedParity
}: {
  connectionDefaults: SshConnectionDefaults
  form: CreateProfileInput
  groupOptions: string[]
  hasSavedPassword: boolean
  hasSavedSuPassword: boolean
  hasSavedSudoPassword: boolean
  isMacOs: boolean
  isLoadingSerialPorts: boolean
  isNetworkDevice: boolean
  mode: ConnectionFormMode
  onClearHostFingerprint?(): void
  refreshSerialPorts(): Promise<void>
  serialDevicePathPlaceholder: string
  serialPortLoadError: string | null
  serialPortOptions: Array<{ value: string; label: string }>
  serialPorts: SerialPortInfo[]
  setForm: ConnectionFormSetter
  showsNetworkDeviceVendor: boolean
  supportsBuiltInRs485: boolean
  supportsExtendedParity: boolean
}) {
  return (
    <div className="ssh-form-page">
      <ConnectionSshGeneralSection
        form={form}
        groupOptions={groupOptions}
        isMacOs={isMacOs}
        isLoadingSerialPorts={isLoadingSerialPorts}
        isNetworkDevice={isNetworkDevice}
        serialDevicePathPlaceholder={serialDevicePathPlaceholder}
        serialPortLoadError={serialPortLoadError}
        serialPortOptions={serialPortOptions}
        serialPorts={serialPorts}
        showsNetworkDeviceVendor={showsNetworkDeviceVendor}
        supportsBuiltInRs485={supportsBuiltInRs485}
        supportsExtendedParity={supportsExtendedParity}
        refreshSerialPorts={refreshSerialPorts}
        setForm={setForm}
      />
      <ConnectionSshAuthSection
        connectionDefaults={connectionDefaults}
        form={form}
        hasSavedPassword={hasSavedPassword}
        hasSavedSuPassword={hasSavedSuPassword}
        hasSavedSudoPassword={hasSavedSudoPassword}
        isNetworkDevice={isNetworkDevice}
        mode={mode}
        onClearHostFingerprint={onClearHostFingerprint}
        setForm={setForm}
      />
    </div>
  )
}
