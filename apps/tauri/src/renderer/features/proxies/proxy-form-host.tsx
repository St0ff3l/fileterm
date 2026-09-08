import { useMemo, useState } from 'react'
import type { CreateProxyProfileInput, UpdateProxyProfileInput } from '@fileterm/core'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { ProxyEditDialog } from './proxy-edit-dialog'

export function ProxyFormHost({
  mode,
  proxyId,
  initialType = 'socks5',
  standalone = false,
  onClose
}: {
  mode: 'create' | 'edit'
  proxyId?: string | null
  initialType?: 'socks5' | 'http'
  standalone?: boolean
  onClose(): void
}) {
  const { proxies, loading, saveProxy, testProxy } = useProxyLibrary()
  const [isSubmitting, setIsSubmitting] = useState(false)

  const editingProxy = useMemo(() => {
    if (mode !== 'edit' || !proxyId) return null
    return proxies.find((p) => p.id === proxyId) ?? null
  }, [mode, proxyId, proxies])

  const handleSave = async (input: CreateProxyProfileInput | UpdateProxyProfileInput) => {
    setIsSubmitting(true)
    try {
      await saveProxy(input)
      onClose()
    } finally {
      setIsSubmitting(false)
    }
  }

  // If in edit mode and still loading the proxy profile from desktopApi, wait briefly
  if (mode === 'edit' && proxyId && loading && !editingProxy) {
    return (
      <div className="connection-form-window">
        <div className="modal-card ssh-modal standalone" style={{ alignItems: 'center', justifyContent: 'center' }}>
          <span className="button-spinner" />
        </div>
      </div>
    )
  }

  return (
    <ProxyEditDialog
      initialProxy={editingProxy}
      initialType={initialType}
      isOpen={true}
      isSubmitting={isSubmitting}
      standalone={standalone}
      onClose={onClose}
      onSave={handleSave}
      onTest={testProxy}
    />
  )
}
