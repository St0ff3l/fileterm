import { useEffect, useLayoutEffect, useState } from 'react'
import { DEFAULT_WORKSPACE_MODE, type FileTermDesktopApi, type WorkspaceMode } from '@fileterm/core'

function normalizeWorkspaceMode(value: unknown): WorkspaceMode {
  return value === 'server-management' ? 'server-management' : DEFAULT_WORKSPACE_MODE
}

export function useWorkspaceMode(desktopApi: FileTermDesktopApi | undefined, initialMode?: WorkspaceMode) {
  const [workspaceMode, setWorkspaceMode] = useState(() => normalizeWorkspaceMode(initialMode))
  const hasInitialMode = initialMode !== undefined

  useLayoutEffect(() => {
    if (initialMode !== undefined) setWorkspaceMode(normalizeWorkspaceMode(initialMode))
  }, [initialMode])

  useEffect(() => {
    if (!desktopApi) return

    let canceled = false
    if (!hasInitialMode) {
      void desktopApi
        .getUiPreferences()
        .then((preferences) => {
          if (!canceled) setWorkspaceMode(normalizeWorkspaceMode(preferences.workspaceMode))
        })
        .catch(() => undefined)
    }
    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) setWorkspaceMode(normalizeWorkspaceMode(preferences.workspaceMode))
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi, hasInitialMode])

  return [workspaceMode, setWorkspaceMode] as const
}
