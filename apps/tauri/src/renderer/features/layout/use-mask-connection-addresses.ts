import { useEffect, useState } from 'react'
import { DEFAULT_MASK_CONNECTION_ADDRESSES, type FileTermDesktopApi } from '@fileterm/core'

function normalizeMaskConnectionAddresses(value: unknown): boolean {
  return typeof value === 'boolean' ? value : DEFAULT_MASK_CONNECTION_ADDRESSES
}

/**
 * Whether the connection summary obscures its private IP and access address,
 * kept current from preference events. Only the settings page writes it; the
 * setter is for its optimistic updates.
 */
export function useMaskConnectionAddresses(
  desktopApi: FileTermDesktopApi | undefined,
  initialValue?: boolean
): readonly [boolean, (nextValue: boolean) => void] {
  const [maskConnectionAddresses, setMaskConnectionAddresses] = useState(() =>
    normalizeMaskConnectionAddresses(initialValue)
  )
  const hasInitialValue = initialValue !== undefined

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    let canceled = false
    // Startup normally hands over the saved value; read it when that failed.
    if (!hasInitialValue) {
      void desktopApi
        .getUiPreferences()
        .then((preferences) => {
          if (!canceled)
            setMaskConnectionAddresses(normalizeMaskConnectionAddresses(preferences.maskConnectionAddresses))
        })
        .catch(() => {
          // Keep addresses visible when the preference cannot be read.
        })
    }
    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) setMaskConnectionAddresses(normalizeMaskConnectionAddresses(preferences.maskConnectionAddresses))
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi, hasInitialValue])

  return [maskConnectionAddresses, setMaskConnectionAddresses] as const
}
