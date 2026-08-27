import { useCallback, useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, SecuritySettings } from '@fileterm/core'

export type SessionSecurityStatus = 'disabled' | 'loading' | 'ready' | 'error'

export function useSessionSecurity(desktopApi: FileTermDesktopApi | undefined, enabled: boolean) {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [status, setStatus] = useState<SessionSecurityStatus>(() => (desktopApi && enabled ? 'loading' : 'disabled'))
  const [isLocked, setIsLocked] = useState(false)
  const [reloadToken, setReloadToken] = useState(0)
  const settingsRef = useRef<SecuritySettings | null>(null)
  const hasLoadedOnceRef = useRef(false)

  const lock = useCallback(() => {
    const current = settingsRef.current
    if (!current?.lockEnabled || !current.hasLockPassword) {
      return
    }
    setIsLocked(true)
    const activeElement = document.activeElement
    if (activeElement instanceof HTMLElement) {
      activeElement.blur()
    }
  }, [])

  useEffect(() => {
    let canceled = false
    hasLoadedOnceRef.current = false
    settingsRef.current = null
    setSettings(null)
    setIsLocked(false)

    if (!desktopApi || !enabled) {
      setStatus('disabled')
      return () => {
        canceled = true
      }
    }

    setStatus('loading')
    const applySettings = (next: SecuritySettings) => {
      settingsRef.current = next
      setSettings(next)
      if (!hasLoadedOnceRef.current) {
        hasLoadedOnceRef.current = true
        if (next.lockEnabled && next.hasLockPassword) {
          setIsLocked(true)
        }
      } else if (!next.lockEnabled || !next.hasLockPassword) {
        setIsLocked(false)
      }
    }
    const unsubscribe = desktopApi.onSecuritySettingsChanged((next) => {
      if (!canceled) {
        applySettings(next)
      }
    })

    void desktopApi
      .getSecuritySettings()
      .then((next) => {
        if (canceled) return
        applySettings(next)
        setStatus('ready')
      })
      .catch(() => {
        if (!canceled) {
          setStatus('error')
        }
      })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi, enabled, reloadToken])

  useEffect(() => {
    const current = settings
    if (
      status !== 'ready' ||
      !current?.lockEnabled ||
      !current.hasLockPassword ||
      current.idleLockMinutes <= 0 ||
      isLocked
    ) {
      return
    }

    const timeoutMs = current.idleLockMinutes * 60_000
    let timer: number | undefined
    const resetTimer = () => {
      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
      timer = window.setTimeout(lock, timeoutMs)
    }
    const activityEvents: Array<keyof WindowEventMap> = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart']
    activityEvents.forEach((eventName) => window.addEventListener(eventName, resetTimer))
    resetTimer()

    return () => {
      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
      activityEvents.forEach((eventName) => window.removeEventListener(eventName, resetTimer))
    }
  }, [isLocked, lock, settings, status])

  const unlock = useCallback(
    async (password: string) => {
      if (!desktopApi || !password) {
        return false
      }
      try {
        const valid = await desktopApi.verifySecurityPassword(password)
        if (!valid) {
          return false
        }
        setIsLocked(false)
        return true
      } catch {
        return false
      }
    },
    [desktopApi]
  )

  const retry = useCallback(() => {
    setReloadToken((token) => token + 1)
  }, [])

  return {
    settings,
    status,
    isLocked,
    lock,
    unlock,
    retry
  }
}
