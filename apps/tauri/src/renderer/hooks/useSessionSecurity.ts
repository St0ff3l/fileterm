import { useCallback, useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, SecuritySettings } from '@fileterm/core'

export type SessionSecurityStatus = 'disabled' | 'loading' | 'ready' | 'error'
type SessionLockReason = 'idle' | 'manual'

function sameSecuritySettings(left: SecuritySettings | null, right: SecuritySettings) {
  return (
    left !== null &&
    left.lockEnabled === right.lockEnabled &&
    left.idleLockMinutes === right.idleLockMinutes &&
    left.hasLockPassword === right.hasLockPassword &&
    left.hasBackupPassword === right.hasBackupPassword
  )
}

export function useSessionSecurity(desktopApi: FileTermDesktopApi | undefined, enabled: boolean) {
  const [settings, setSettings] = useState<SecuritySettings | null>(null)
  const [status, setStatus] = useState<SessionSecurityStatus>(() => (desktopApi && enabled ? 'loading' : 'disabled'))
  const [isLocked, setIsLocked] = useState(false)
  const [reloadToken, setReloadToken] = useState(0)
  const settingsRef = useRef<SecuritySettings | null>(null)
  const hasLoadedOnceRef = useRef(false)
  const settingsRevisionRef = useRef(0)
  const idleTimerGenerationRef = useRef(0)

  const lock = useCallback((force = false, reason: SessionLockReason = 'manual') => {
    const current = settingsRef.current
    if (!current?.hasLockPassword) return
    const autoLockEnabled = reason === 'idle' ? current.idleLockMinutes > 0 : current.lockEnabled
    if (!force && !autoLockEnabled) return
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
    settingsRevisionRef.current = 0
    idleTimerGenerationRef.current += 1
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
      if (sameSecuritySettings(settingsRef.current, next)) {
        return
      }
      settingsRevisionRef.current += 1
      settingsRef.current = next
      setSettings(next)
      if (!hasLoadedOnceRef.current) {
        hasLoadedOnceRef.current = true
        if (next.lockEnabled && next.hasLockPassword) {
          setIsLocked(true)
        }
      } else if (!next.hasLockPassword) {
        setIsLocked(false)
      }
    }
    const initialLoadRevision = settingsRevisionRef.current
    const unsubscribe = desktopApi.onSecuritySettingsChanged((next) => {
      if (!canceled) {
        applySettings(next)
      }
    })

    void desktopApi
      .getSecuritySettings()
      .then((next) => {
        if (canceled) return
        // A live settings event can win the race with this initial read. Do
        // not let the older read restore a one-minute timer after “Never” was
        // already applied.
        if (settingsRevisionRef.current === initialLoadRevision) {
          applySettings(next)
        }
        setStatus('ready')
      })
      .catch(() => {
        if (!canceled) {
          setStatus(settingsRef.current ? 'ready' : 'error')
        }
      })

    return () => {
      canceled = true
      unsubscribe()
      idleTimerGenerationRef.current += 1
    }
  }, [desktopApi, enabled, reloadToken])

  useEffect(() => {
    const current = settings
    const timerGeneration = idleTimerGenerationRef.current + 1
    idleTimerGenerationRef.current = timerGeneration
    if (status !== 'ready' || !current?.hasLockPassword || current.idleLockMinutes <= 0 || isLocked) {
      return
    }

    const timeoutMs = current.idleLockMinutes * 60_000
    let timer: number | undefined
    const resetTimer = () => {
      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
      timer = window.setTimeout(() => {
        // React cleanup normally clears this timer when settings change, but
        // a callback can already be queued in the browser task queue. The
        // generation and object checks make an obsolete timer harmless.
        if (idleTimerGenerationRef.current !== timerGeneration || settingsRef.current !== current) {
          return
        }
        lock(false, 'idle')
      }, timeoutMs)
    }
    const activityEvents: Array<keyof WindowEventMap> = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart']
    activityEvents.forEach((eventName) => window.addEventListener(eventName, resetTimer))
    resetTimer()

    return () => {
      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
      activityEvents.forEach((eventName) => window.removeEventListener(eventName, resetTimer))
      if (idleTimerGenerationRef.current === timerGeneration) {
        idleTimerGenerationRef.current += 1
      }
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

  const lockNow = useCallback(() => {
    lock(true)
  }, [lock])

  const retry = useCallback(() => {
    setReloadToken((token) => token + 1)
  }, [])

  return {
    settings,
    status,
    isLocked,
    lock,
    lockNow,
    unlock,
    retry
  }
}
