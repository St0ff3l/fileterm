import type { ConnectionFormMode, ConnectionProfile, CreateProfileInput } from '@fileterm/core'
import type { FormEvent } from 'react'
import { ConnectionModal } from './ConnectionModal'

export function ConnectionFormHost({
  editingProfileId,
  errorMessage,
  form,
  groupOptions,
  mode,
  profiles,
  setForm,
  standalone,
  onClearHostFingerprint,
  onClose,
  onSubmit
}: {
  editingProfileId: string | null
  errorMessage: string | null
  form: CreateProfileInput
  groupOptions: string[]
  mode: ConnectionFormMode
  profiles: ConnectionProfile[]
  setForm(updater: CreateProfileInput | ((current: CreateProfileInput) => CreateProfileInput)): void
  standalone?: boolean
  onClearHostFingerprint(profile: ConnectionProfile): void
  onClose(): void
  onSubmit(event: FormEvent<HTMLFormElement>): void
}) {
  const clearHostFingerprint = () => {
    const editingProfile = editingProfileId
      ? (profiles.find((profile) => profile.id === editingProfileId) ?? null)
      : null
    if (!editingProfile) {
      return
    }
    onClearHostFingerprint(editingProfile)
    setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
  }

  return (
    <ConnectionModal
      errorMessage={errorMessage}
      groupOptions={groupOptions}
      mode={mode}
      form={form}
      profiles={profiles}
      setForm={setForm}
      onClearHostFingerprint={clearHostFingerprint}
      standalone={standalone}
      onSubmit={onSubmit}
      onClose={onClose}
    />
  )
}
