import { useCallback, useEffect, useState } from 'react'
import type { SshKeyMetadata } from '@fileterm/core'

export function useSshKeyLibrary() {
  const desktopApi = window.fileterm
  const [keys, setKeys] = useState<SshKeyMetadata[]>([])
  const [loading, setLoading] = useState(Boolean(desktopApi))
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!desktopApi) return
    try {
      setKeys(await desktopApi.listSshKeys())
      setError(null)
    } catch (nextError) {
      setError(errorMessage(nextError))
    } finally {
      setLoading(false)
    }
  }, [desktopApi])

  useEffect(() => {
    void refresh()
    return desktopApi?.onSshKeysChanged((nextKeys) => {
      setKeys(nextKeys)
      setLoading(false)
      setError(null)
    })
  }, [desktopApi, refresh])

  const importKey = useCallback(
    async (note = '', sourcePath?: string) => {
      if (!desktopApi) return null
      try {
        const result = await desktopApi.importSshKey({ note, sourcePath })
        if (result) {
          setKeys((current) => [result.key, ...current.filter((key) => key.id !== result.key.id)])
        }
        setError(null)
        return result
      } catch (nextError) {
        setError(errorMessage(nextError))
        throw nextError
      }
    },
    [desktopApi]
  )

  const updateNote = useCallback(
    async (keyId: string, note: string) => {
      if (!desktopApi) return
      try {
        const updated = await desktopApi.updateSshKeyNote(keyId, note)
        setKeys((current) => current.map((key) => (key.id === keyId ? updated : key)))
        setError(null)
      } catch (nextError) {
        setError(errorMessage(nextError))
        throw nextError
      }
    },
    [desktopApi]
  )

  const deleteKey = useCallback(
    async (keyId: string) => {
      if (!desktopApi) return
      try {
        await desktopApi.deleteSshKey(keyId)
        setKeys((current) => current.filter((key) => key.id !== keyId))
        setError(null)
      } catch (nextError) {
        setError(errorMessage(nextError))
        throw nextError
      }
    },
    [desktopApi]
  )

  return { keys, loading, error, refresh, importKey, updateNote, deleteKey }
}

function errorMessage(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  return message.replace(/Error invoking remote method '[^']+':\s*/i, '').trim()
}
