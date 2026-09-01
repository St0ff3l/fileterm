import { useEffect } from 'react'
import {
  DEFAULT_LOCAL_TERMINAL_SHELLS,
  DEFAULT_MCP_AGENT_PREFERENCES,
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  type UiPreferences
} from '@fileterm/core'
import { sameOverviewSectionOrder, SETTINGS_SIDEBAR_ITEMS, type SettingsTab } from '../constants'
import { registerImportedFonts } from '../../../../app/imported-fonts'
import { t } from '../../../../i18n'
import type { SettingsModalState } from './state'

export function useSettingsModalEffects({
  state,
  initialTab,
  visibleSettingsTabs
}: {
  state: SettingsModalState
  initialTab: SettingsTab
  visibleSettingsTabs: Set<SettingsTab>
}) {
  const {
    activeTab,
    setActiveTab,
    settingsSearchQuery,
    desktopApi,
    updatePreviewState,
    setUpdateStatus,
    setAutoCheckUpdates,
    setUpdateChannel,
    setTerminalZoomLocked,
    setLocalTerminalShells,
    setLocalTerminalShellDrafts,
    setFilePanelRememberRatio,
    setMcpAgentPreferences,
    setConnectionDefaults,
    setOverviewShowStats,
    setOverviewShowRecent,
    setOverviewShowAllConnections,
    setOverviewShowQuickActions,
    setOverviewSectionOrder,
    setUpdatePreferenceError,
    setImportedFonts,
    setFontImportError,
    localTerminalShellScanVersion,
    setLocalTerminalShellOptions,
    setIsLoadingLocalTerminalShellOptions,
    setLocalTerminalShellError
  } = state

  useEffect(() => {
    if (!desktopApi) return

    let canceled = false
    void desktopApi
      .listImportedFonts()
      .then(async (fonts) => {
        const entries = await Promise.all(
          fonts.map(async (font) => {
            const dataUrl = await desktopApi.getImportedFontData(font.id)
            return dataUrl ? { font, dataUrl } : null
          })
        )
        if (canceled) return
        setImportedFonts(fonts)
        registerImportedFonts(
          entries.filter((entry): entry is { font: (typeof fonts)[number]; dataUrl: string } => entry !== null)
        )
      })
      .catch((cause: unknown) => {
        console.error('[FileTerm] 加载导入字体', cause)
        if (!canceled) setFontImportError(t.themeFontImportFailed)
      })

    return () => {
      canceled = true
    }
  }, [desktopApi])

  useEffect(() => {
    setActiveTab(initialTab)
  }, [initialTab])

  useEffect(() => {
    if (!settingsSearchQuery.trim() || visibleSettingsTabs.has(activeTab)) {
      return
    }

    const nextTab = SETTINGS_SIDEBAR_ITEMS.find((item) => visibleSettingsTabs.has(item.tab))?.tab
    if (nextTab) {
      setActiveTab(nextTab)
    }
  }, [activeTab, settingsSearchQuery, visibleSettingsTabs])

  useEffect(() => {
    if (updatePreviewState) {
      setUpdateStatus({
        currentVersion: desktopApi?.appVersion ?? '1.0.0',
        state:
          updatePreviewState === 'downloading' || updatePreviewState === 'downloaded' || updatePreviewState === 'error'
            ? updatePreviewState
            : 'available',
        availableVersion: '1.1.0',
        progress: updatePreviewState === 'downloading' ? 62 : updatePreviewState === 'downloaded' ? 100 : undefined,
        message: updatePreviewState === 'error' ? t.updateServerUnavailable : undefined
      })
      return
    }
    if (!desktopApi) {
      return
    }
    void desktopApi.getUpdateStatus().then(setUpdateStatus)
    return desktopApi.onUpdateStatus(setUpdateStatus)
  }, [desktopApi, updatePreviewState])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    let canceled = false
    const applyUiPreferences = (preferences: UiPreferences) => {
      if (canceled) return
      setAutoCheckUpdates(preferences.autoCheckUpdates)
      setUpdateChannel(preferences.updateChannel)
      setTerminalZoomLocked(preferences.terminalZoomLocked)
      setLocalTerminalShells({ ...DEFAULT_LOCAL_TERMINAL_SHELLS, ...preferences.localTerminalShells })
      setLocalTerminalShellDrafts({ ...DEFAULT_LOCAL_TERMINAL_SHELLS, ...preferences.localTerminalShells })
      setFilePanelRememberRatio(preferences.filePanelRememberRatio)
      setMcpAgentPreferences({ ...DEFAULT_MCP_AGENT_PREFERENCES, ...preferences.mcpAgent })
      setConnectionDefaults({ ...DEFAULT_SSH_CONNECTION_DEFAULTS, ...preferences.connectionDefaults })
      setOverviewShowStats(preferences.overviewShowStats)
      setOverviewShowRecent(preferences.overviewShowRecent)
      setOverviewShowAllConnections(preferences.overviewShowAllConnections)
      setOverviewShowQuickActions(preferences.overviewShowQuickActions)
      setOverviewSectionOrder((currentOrder) =>
        sameOverviewSectionOrder(currentOrder, preferences.overviewSectionOrder)
          ? currentOrder
          : preferences.overviewSectionOrder
      )
    }

    void desktopApi
      .getUiPreferences()
      .then(applyUiPreferences)
      .catch(() => {
        if (!canceled) {
          setUpdatePreferenceError(t.updatePreferenceLoadFailed)
        }
      })

    const unsubscribe = desktopApi.onUiPreferencesChanged(applyUiPreferences)

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi])

  useEffect(() => {
    if (activeTab !== 'local-terminal' || !desktopApi) {
      return
    }

    let canceled = false
    setIsLoadingLocalTerminalShellOptions(true)
    setLocalTerminalShellError(null)
    void desktopApi
      .listLocalTerminalShells()
      .then((options) => {
        if (!canceled) {
          setLocalTerminalShellOptions(options)
        }
      })
      .catch(() => {
        if (!canceled) {
          setLocalTerminalShellError(t.localTerminalShellDetectionFailed)
        }
      })
      .finally(() => {
        if (!canceled) {
          setIsLoadingLocalTerminalShellOptions(false)
        }
      })

    return () => {
      canceled = true
    }
  }, [activeTab, desktopApi, localTerminalShellScanVersion])
}
