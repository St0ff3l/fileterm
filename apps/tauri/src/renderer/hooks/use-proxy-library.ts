import { useCallback, useEffect, useState } from 'react'
import type { CreateProxyProfileInput, ProxyProfile, UpdateProxyProfileInput } from '@fileterm/core'

export function useProxyLibrary() {
  const desktopApi = window.fileterm
  const [proxies, setProxies] = useState<ProxyProfile[]>([])
  const [loading, setLoading] = useState(Boolean(desktopApi))
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!desktopApi) return
    try {
      setProxies(await desktopApi.listProxyProfiles())
      setError(null)
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setLoading(false)
    }
  }, [desktopApi])

  useEffect(() => {
    void refresh()
    const stop = desktopApi?.onProxiesChanged?.(() => {
      void refresh()
    })
    return () => {
      stop?.()
    }
  }, [refresh, desktopApi])

  const saveProxy = useCallback(
    async (input: CreateProxyProfileInput | UpdateProxyProfileInput) => {
      if (!desktopApi) throw new Error('Desktop API unavailable')
      try {
        const saved = await desktopApi.saveProxyProfile(input)
        setProxies((current) => {
          const index = current.findIndex((p) => p.id === saved.id)
          if (index >= 0) {
            const next = [...current]
            next[index] = saved
            return next
          }
          return [saved, ...current]
        })
        setError(null)
        return saved
      } catch (nextError) {
        setError(errorMessage(nextError))
        throw nextError
      }
    },
    [desktopApi]
  )

  const deleteProxy = useCallback(
    async (id: string) => {
      if (!desktopApi) return
      try {
        await desktopApi.deleteProxyProfile(id)
        setProxies((current) => current.filter((p) => p.id !== id))
        setError(null)
      } catch (nextError) {
        setError(errorMessage(nextError))
        throw nextError
      }
    },
    [desktopApi]
  )

  const testProxy = useCallback(
    async (input: CreateProxyProfileInput | { id: string }) => {
      if (!desktopApi) throw new Error('Desktop API unavailable')
      try {
        return await desktopApi.testProxyProfile(input)
      } catch (nextError) {
        return { success: false, error: errorMessage(nextError) }
      }
    },
    [desktopApi]
  )

  const clearError = useCallback(() => setError(null), [])

  return {
    proxies,
    loading,
    error,
    clearError,
    refresh,
    saveProxy,
    deleteProxy,
    testProxy
  }
}

function errorMessage(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  return message.replace(/Error invoking remote method '[^']+':\s*/i, '').trim()
}
