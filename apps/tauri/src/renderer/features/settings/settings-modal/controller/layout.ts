import { useState } from 'react'
import type { WorkspaceMode } from '@fileterm/core'
import { t } from '../../../../i18n'
import { useCompactTabBar } from '../../../layout/use-compact-tab-bar'
import { useWorkspaceMode } from '../../../layout/use-workspace-mode'
import type { SettingsModalState } from './state'

export function useLayoutSettingsController({ state }: { state: SettingsModalState }) {
  const { desktopApi } = state
  const [workspaceMode, setWorkspaceMode] = useWorkspaceMode(desktopApi)
  const [isSavingWorkspaceMode, setIsSavingWorkspaceMode] = useState(false)
  const [workspaceModeError, setWorkspaceModeError] = useState<string | null>(null)
  const [compactTabBar, setCompactTabBar] = useCompactTabBar(desktopApi)
  const [isSavingCompactTabBar, setIsSavingCompactTabBar] = useState(false)
  const [compactTabBarError, setCompactTabBarError] = useState<string | null>(null)

  const setWorkspaceModePreference = (nextValue: WorkspaceMode) => {
    if (!desktopApi || isSavingWorkspaceMode || nextValue === workspaceMode) {
      return
    }

    const previousValue = workspaceMode
    setWorkspaceMode(nextValue)
    setWorkspaceModeError(null)
    setIsSavingWorkspaceMode(true)
    void desktopApi
      .setUiPreferences({ workspaceMode: nextValue })
      .then((preferences) => setWorkspaceMode(preferences.workspaceMode))
      .catch(() => {
        setWorkspaceMode(previousValue)
        setWorkspaceModeError(t.workspaceModeSaveFailed)
      })
      .finally(() => setIsSavingWorkspaceMode(false))
  }

  const setCompactTabBarPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingCompactTabBar || nextValue === compactTabBar) {
      return
    }

    const previousValue = compactTabBar
    setCompactTabBar(nextValue)
    setCompactTabBarError(null)
    setIsSavingCompactTabBar(true)
    void desktopApi
      .setUiPreferences({ compactTabBar: nextValue })
      .then((preferences) => setCompactTabBar(preferences.compactTabBar))
      .catch(() => {
        setCompactTabBar(previousValue)
        setCompactTabBarError(t.compactTabBarSaveFailed)
      })
      .finally(() => setIsSavingCompactTabBar(false))
  }

  return {
    workspaceMode,
    isSavingWorkspaceMode,
    workspaceModeError,
    setWorkspaceModePreference,
    compactTabBar,
    isSavingCompactTabBar,
    compactTabBarError,
    setCompactTabBarPreference
  }
}
