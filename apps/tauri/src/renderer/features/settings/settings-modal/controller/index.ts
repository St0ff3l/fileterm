import { useMemo } from 'react'
import { t } from '../../../../i18n'
import { getWebLoginStatus } from '../../../../../bridge/web-login'
import { SETTINGS_SIDEBAR_ITEMS, SETTINGS_TAB_SEARCH_TERMS } from '../constants'
import { useAiSettingsController } from './ai'
import { useAgentSettingsController } from './agent'
import { useSettingsModalEffects } from './effects'
import { useLayoutSettingsController } from './layout'
import { useOverviewSettingsController } from './overview'
import { useSettingsPreferencesController } from './preferences'
import { useSettingsSecurityController } from './security'
import { useSyncSettingsController } from './sync'
import { useSettingsModalState } from './state'
import type { SettingsModalControllerOptions } from './types'
import { useThemeSettingsController } from './theme'

export type { SettingsModalControllerOptions } from './types'

export function useSettingsModalController(options: SettingsModalControllerOptions) {
  const { locale, initialTab, inline, onLaunchLocalAgent, uiPreferences } = options
  const state = useSettingsModalState(initialTab, uiPreferences)
  const { activeTab, settingsSearchQuery, desktopApi } = state

  const dockerWebLogin = getWebLoginStatus() !== null
  const visibleSettingsTabs = useMemo(() => {
    const items = SETTINGS_SIDEBAR_ITEMS.filter((item) => item.tab !== 'web-login' || dockerWebLogin)
    const query = settingsSearchQuery.trim().toLocaleLowerCase()
    if (!query) {
      return new Set(items.map((item) => item.tab))
    }

    return new Set(
      items
        .filter((item) => {
          const searchText = `${t[item.labelKey]} ${SETTINGS_TAB_SEARCH_TERMS[item.tab]}`.toLocaleLowerCase()
          return searchText.includes(query)
        })
        .map((item) => item.tab)
    )
  }, [dockerWebLogin, locale, settingsSearchQuery])

  const preferences = useSettingsPreferencesController({ state })
  const theme = useThemeSettingsController({
    state,
    options
  })
  const overview = useOverviewSettingsController({ state })
  const layout = useLayoutSettingsController({
    state,
    initialWorkspaceMode: options.workspaceMode,
    uiPreferences
  })
  const ai = useAiSettingsController({ state, desktopApi })
  const agent = useAgentSettingsController({ state, onLaunchLocalAgent })
  const security = useSettingsSecurityController({ state })
  const sync = useSyncSettingsController({ state, openSecuritySettings: security.openSecuritySettings })

  useSettingsModalEffects({ state, initialTab, visibleSettingsTabs, uiPreferences })

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
    ...layout,
    ...ai,
    ...agent,
    ...sync,
    openSecuritySettings: security.openSecuritySettings
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
