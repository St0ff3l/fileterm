import type { FileTermDesktopApi } from '@fileterm/core'
import { useCallback, useRef, useState } from 'react'

export function useExternalUrl(api: FileTermDesktopApi | undefined, failureLabel: string) {
  const [externalUrlError, setExternalUrlError] = useState<string | null>(null)
  const opening = useRef(false)
  const openExternalUrl = useCallback(
    async (url: string) => {
      if (!api || opening.current) return
      opening.current = true
      setExternalUrlError(null)
      try {
        await api.openExternalUrl(url)
      } catch (error) {
        setExternalUrlError(`${failureLabel}: ${error instanceof Error ? error.message : String(error)}`)
      } finally {
        opening.current = false
      }
    },
    [api, failureLabel]
  )
  return { openExternalUrl, externalUrlError }
}
