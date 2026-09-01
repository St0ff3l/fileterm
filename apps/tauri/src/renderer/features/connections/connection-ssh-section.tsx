import type {
  ConnectionFormMode,
  ConnectionProfile,
  CreateProfileInput,
  ResourceMonitoringMetric,
  SerialPortInfo,
  SshConnectionDefaults
} from '@fileterm/core'
import { ConnectionSshAdvancedSection } from './connection-ssh-advanced-section'
import { ConnectionSshAuthSection } from './connection-ssh-auth-section'
import { ConnectionSshGeneralSection } from './connection-ssh-general-section'
import type { ConnectionFormSetter, SshConnectionSettingKey } from './connection-modal-utils'

export function ConnectionSshSection({
  connectionDefaults,
  fallbackResourceMonitoringMetrics,
  fallbackResourceMonitoringMetricOrder,
  form,
  groupOptions,
  hasSavedPassword,
  hasSavedSuPassword,
  hasSavedSudoPassword,
  intervalSettingOptions,
  isMacOs,
  isLoadingSerialPorts,
  isNetworkDevice,
  isSubmitting,
  jumpHosts,
  mode,
  onClearHostFingerprint,
  refreshSerialPorts,
  routingMode,
  serialDevicePathPlaceholder,
  serialPortLoadError,
  serialPortOptions,
  serialPorts,
  setForm,
  setRoutingMode,
  setSshConnectionSetting,
  showsNetworkDeviceVendor,
  supportsBuiltInRs485,
  supportsExtendedParity
}: {
  connectionDefaults: SshConnectionDefaults
  fallbackResourceMonitoringMetrics?: ResourceMonitoringMetric[]
  fallbackResourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  form: CreateProfileInput
  groupOptions: string[]
  hasSavedPassword: boolean
  hasSavedSuPassword: boolean
  hasSavedSudoPassword: boolean
  intervalSettingOptions: Array<{ value: string; label: string }>
  isMacOs: boolean
  isLoadingSerialPorts: boolean
  isNetworkDevice: boolean
  isSubmitting: boolean
  jumpHosts: ConnectionProfile[]
  mode: ConnectionFormMode
  onClearHostFingerprint?(): void
  refreshSerialPorts(): Promise<void>
  routingMode: 'direct' | 'jump'
  serialDevicePathPlaceholder: string
  serialPortLoadError: string | null
  serialPortOptions: Array<{ value: string; label: string }>
  serialPorts: SerialPortInfo[]
  setForm: ConnectionFormSetter
  setRoutingMode(value: 'direct' | 'jump'): void
  setSshConnectionSetting<K extends SshConnectionSettingKey>(key: K, value: SshConnectionDefaults[K]): void
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
      <ConnectionSshAdvancedSection
        connectionDefaults={connectionDefaults}
        fallbackResourceMonitoringMetrics={fallbackResourceMonitoringMetrics}
        fallbackResourceMonitoringMetricOrder={fallbackResourceMonitoringMetricOrder}
        form={form}
        intervalSettingOptions={intervalSettingOptions}
        isNetworkDevice={isNetworkDevice}
        isSubmitting={isSubmitting}
        jumpHosts={jumpHosts}
        routingMode={routingMode}
        setForm={setForm}
        setRoutingMode={setRoutingMode}
        setSshConnectionSetting={setSshConnectionSetting}
      />
    </div>
  )
}
