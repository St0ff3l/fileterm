import { useMemo } from 'react'
import { t } from '../../../../i18n'
import { SETTINGS_SIDEBAR_ITEMS, SETTINGS_TAB_SEARCH_TERMS } from '../constants'
import { useAiSettingsController } from './ai'
import { useAgentSettingsController } from './agent'
import { useSettingsModalEffects } from './effects'
import { useOverviewSettingsController } from './overview'
import { useSettingsPreferencesController } from './preferences'
import { useSettingsSecurityController } from './security'
import { useSyncSettingsController } from './sync'
import { useSettingsModalState } from './state'
import type { SettingsModalControllerOptions } from './types'
import { useThemeSettingsController } from './theme'

export type { SettingsModalControllerOptions } from './types'

export function useSettingsModalController(options: SettingsModalControllerOptions) {
  const { locale, initialTab, inline, onLaunchLocalAgent } = options
  const state = useSettingsModalState(initialTab)
  const { activeTab, settingsSearchQuery, desktopApi } = state

  const visibleSettingsTabs = useMemo(() => {
    const query = settingsSearchQuery.trim().toLocaleLowerCase()
    if (!query) {
      return new Set(SETTINGS_SIDEBAR_ITEMS.map((item) => item.tab))
    }

    return new Set(
      SETTINGS_SIDEBAR_ITEMS.filter((item) => {
        const searchText = `${t[item.labelKey]} ${SETTINGS_TAB_SEARCH_TERMS[item.tab]}`.toLocaleLowerCase()
        return searchText.includes(query)
      }).map((item) => item.tab)
    )
  }, [locale, settingsSearchQuery])

  const preferences = useSettingsPreferencesController({ state })
  const theme = useThemeSettingsController({
    state,
    options
  })
  const overview = useOverviewSettingsController({ state })
  const ai = useAiSettingsController({ state, desktopApi })
  const agent = useAgentSettingsController({ state, onLaunchLocalAgent })
  const security = useSettingsSecurityController({ state })
  const sync = useSyncSettingsController({ state, openSecuritySettings: security.openSecuritySettings })

  useSettingsModalEffects({ state, initialTab, visibleSettingsTabs })

  const managerToolsHint = inline ? t.settingsManagersInlineHint : t.settingsManagersWindowHint
  const managerToolsActionLabel = inline ? t.switchToManagerPage : t.openInSeparateWindow

  const settingsPanelContext = {
    t,
    desktopApi,
    ...preferences,
    managerToolsHint,
    managerToolsActionLabel,
    onOpenConnectionManager: options.onOpenConnectionManager,
    onOpenCommandManager: options.onOpenCommandManager,
    onOpenLogsDirectory: options.onOpenLogsDirectory,
    locale,
    onSetLocale: options.onSetLocale,
    ...theme,
    ...overview,
    ...ai,
    ...agent,
    ...sync
  }

  return {
    activeTab,
    setActiveTab: state.setActiveTab,
    settingsSearchQuery,
    setSettingsSearchQuery: state.setSettingsSearchQuery,
    visibleSettingsTabs,
    desktopApi,
    syncOperation: state.syncOperation,
    securityNotice: security.securityNotice,
    setSecurityNotice: security.setSecurityNotice,
    securityFocusRequest: security.securityFocusRequest,
    handleSecurityBackupPasswordFocusHandled: security.handleSecurityBackupPasswordFocusHandled,
    settingsPanelContext
  }
}
