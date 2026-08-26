import type {
  ConnectionFormMode,
  ConnectionProfile,
  CreateProfileInput,
  ResourceMonitoringMetric,
  SshConnectionDefaults
} from '@fileterm/core'
import type { FormEvent } from 'react'
import { ConnectionModal } from './ConnectionModal'

export function ConnectionFormHost({
  editingProfileId,
  errorMessage,
  form,
  connectionDefaults,
  fallbackResourceMonitoringMetrics,
  fallbackResourceMonitoringMetricOrder,
  groupOptions,
  isSubmitting,
  mode,
  profiles,
  setForm,
  standalone,
  onClearHostFingerprint,
  onClose,
  onDismissError,
  onTestConnection,
  onSubmit
}: {
  editingProfileId: string | null
  errorMessage: string | null
  form: CreateProfileInput
  connectionDefaults: SshConnectionDefaults
  fallbackResourceMonitoringMetrics?: ResourceMonitoringMetric[]
  fallbackResourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  groupOptions: string[]
  isSubmitting?: boolean
  mode: ConnectionFormMode
  profiles: ConnectionProfile[]
  setForm(updater: CreateProfileInput | ((current: CreateProfileInput) => CreateProfileInput)): void
  standalone?: boolean
  onClearHostFingerprint(profile: ConnectionProfile): void
  onClose(): void
  onDismissError(): void
  onTestConnection(): Promise<boolean>
  onSubmit(event: FormEvent<HTMLFormElement>): void
}) {
  const editingProfile = editingProfileId ? (profiles.find((profile) => profile.id === editingProfileId) ?? null) : null

  const clearHostFingerprint = () => {
    if (!editingProfile) {
      return
    }
    onClearHostFingerprint(editingProfile)
    setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
  }

  return (
    <ConnectionModal
      errorMessage={errorMessage}
      connectionDefaults={connectionDefaults}
      fallbackResourceMonitoringMetrics={fallbackResourceMonitoringMetrics}
      fallbackResourceMonitoringMetricOrder={fallbackResourceMonitoringMetricOrder}
      groupOptions={groupOptions}
      isSubmitting={isSubmitting}
      mode={mode}
      form={form}
      hasSavedPassword={editingProfile?.hasSavedPassword === true}
      hasSavedSudoPassword={editingProfile?.hasSavedSudoPassword === true}
      hasSavedSuPassword={editingProfile?.hasSavedSuPassword === true}
      profiles={profiles}
      editingProfileId={editingProfileId}
      setForm={setForm}
      onClearHostFingerprint={clearHostFingerprint}
      standalone={standalone}
      onDismissError={onDismissError}
      onTestConnection={onTestConnection}
      onSubmit={onSubmit}
      onClose={onClose}
    />
  )
}
