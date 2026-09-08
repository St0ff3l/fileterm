import { useMemo, useState } from 'react'
import type { CreateTunnelProfileInput, TunnelProfileType, UpdateTunnelProfileInput } from '@fileterm/core'
import { useTunnelLibrary } from '../../hooks/use-tunnel-library'
import { WorkspaceLoadingState } from '../common/workspace-loading-state'
import { TunnelEditDialog } from './tunnel-edit-dialog'
import { t } from '../../i18n'

export function TunnelFormHost({
  mode,
  tunnelId,
  initialType = 'ssh',
  standalone = false,
  onClose
}: {
  mode: 'create' | 'edit'
  tunnelId?: string | null
  initialType?: TunnelProfileType
  standalone?: boolean
  onClose(): void
}) {
  const { tunnels, loading, saveTunnel } = useTunnelLibrary()
  const [isSubmitting, setIsSubmitting] = useState(false)
  const initialTunnel = useMemo(
    () => (mode === 'edit' && tunnelId ? (tunnels.find((tunnel) => tunnel.id === tunnelId) ?? null) : null),
    [mode, tunnelId, tunnels]
  )

  if (mode === 'edit' && loading && !initialTunnel) {
    return <WorkspaceLoadingState label={t.loadingTunnels} />
  }

  return (
    <TunnelEditDialog
      initialTunnel={initialTunnel}
      initialType={initialType}
      isOpen
      isSubmitting={isSubmitting}
      standalone={standalone}
      onClose={onClose}
      onSave={async (input: CreateTunnelProfileInput | UpdateTunnelProfileInput) => {
        setIsSubmitting(true)
        try {
          await saveTunnel(input)
          onClose()
        } finally {
          setIsSubmitting(false)
        }
      }}
    />
  )
}
