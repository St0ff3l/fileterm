import type { McpAgentClientStatus, SavedTheme, ThemeConfig } from '@fileterm/core'
import type { ThemeMode } from '../../../app/theme-config'
import { t } from '../../../i18n'
import { AppIcon } from '../../common/app-icon'
import { CloseButton } from '../../common/close-button'
import { SecuritySettingsPanel } from '../../security/security-settings-panel'
import { SettingsModalProvider } from './context'
import { useSettingsModalController } from './controller'
import { AiSettingsPanel } from './panels/ai-settings-panel'
import { AgentSettingsPanel } from './panels/agent-settings-panel'
import { ConnectionsSettingsPanel } from './panels/connections-settings-panel'
import { InterfaceSettingsPanel } from './panels/interface-settings-panel'
import { LanguageSettingsPanel } from './panels/language-settings-panel'
import { LocalTerminalSettingsPanel } from './panels/local-terminal-settings-panel'
import { SyncSettingsPanel } from './panels/sync-settings-panel'
import { SystemSettingsPanel } from './panels/system-settings-panel'
import { ToolsSettingsPanel } from './panels/tools-settings-panel'
import { UpdatesSettingsPanel } from './panels/updates-settings-panel'

import { SETTINGS_SIDEBAR_ITEMS, type SettingsTab } from './constants'

export function SettingsModal({
  theme,
  themeConfig,
  customThemes,
  onSetTheme,
  onSetThemeConfig,
  onSetCustomThemes,
  locale,
  onSetLocale,
  onOpenCommandManager,
  onOpenConnectionManager,
  onOpenLogsDirectory,
  onLaunchLocalAgent,
  onClose,
  initialTab = 'interface',
  standalone = false,
  inline = false
}: {
  theme: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  onSetTheme(value: ThemeMode): void
  onSetThemeConfig(value: ThemeConfig): void
  onSetCustomThemes(value: SavedTheme[]): void
  locale: 'zhCN' | 'enUS'
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenCommandManager(): void
  onOpenConnectionManager(): void
  onOpenLogsDirectory(): void
  /** Opens an Agent in a visible local terminal; secrets never pass through MCP. */
  onLaunchLocalAgent?(client: McpAgentClientStatus): void
  onClose(): void
  initialTab?: SettingsTab
  standalone?: boolean
  inline?: boolean
}) {
  const {
    activeTab,
    setActiveTab,
    settingsSearchQuery,
    setSettingsSearchQuery,
    visibleSettingsTabs,
    desktopApi,
    syncOperation,
    securityNotice,
    setSecurityNotice,
    securityFocusRequest,
    handleSecurityBackupPasswordFocusHandled,
    settingsPanelContext
  } = useSettingsModalController({
    theme,
    themeConfig,
    customThemes,
    onSetTheme,
    onSetThemeConfig,
    onSetCustomThemes,
    locale,
    onSetLocale,
    onOpenCommandManager,
    onOpenConnectionManager,
    onOpenLogsDirectory,
    onLaunchLocalAgent,
    initialTab,
    inline
  })

  const content = (
    <SettingsModalProvider value={settingsPanelContext}>
      <div
        className={`modal-card manager-modal connection-manager-modal settings-modal ${standalone ? 'standalone' : ''} ${inline ? 'manager-inline' : ''}`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="connection-manager-header">
          <span className="connection-manager-title">
            <span className="material-symbols-outlined">settings</span>
            <span>{t.settings}</span>
          </span>
          <label className="connection-manager-search settings-search">
            <AppIcon name="search" size={14} />
            <input
              aria-label={t.filterSettings}
              placeholder={t.filterSettings}
              type="search"
              value={settingsSearchQuery}
              onChange={(event) => setSettingsSearchQuery(event.target.value)}
            />
          </label>
          {!inline && (
            <div className="connection-manager-header-actions">
              <CloseButton disabled={syncOperation !== null} onClick={onClose} />
            </div>
          )}
        </div>
        <div className="connection-manager-layout">
          <aside className="connection-manager-sidebar" aria-label={t.settings}>
            {SETTINGS_SIDEBAR_ITEMS.filter((item) => visibleSettingsTabs.has(item.tab)).map((item) => (
              <button
                className={`connection-manager-sidebar-item ${activeTab === item.tab ? 'active' : ''}`}
                key={item.tab}
                type="button"
                onClick={() => setActiveTab(item.tab)}
              >
                <span className="connection-manager-sidebar-icon">
                  {item.appIcon ? (
                    <AppIcon name={item.appIcon} size={17} />
                  ) : (
                    <span className="material-symbols-outlined">{item.materialIcon}</span>
                  )}
                </span>
                <span className="connection-manager-sidebar-label">{t[item.labelKey]}</span>
              </button>
            ))}
            {visibleSettingsTabs.size === 0 ? <p className="settings-sidebar-empty">{t.noMatchingSettings}</p> : null}
          </aside>

          <main className="connection-manager-main">
            {activeTab === 'local-terminal' ? <LocalTerminalSettingsPanel /> : null}

            {activeTab === 'ai' ? <AiSettingsPanel /> : null}

            {activeTab === 'agent' ? <AgentSettingsPanel /> : null}

            {activeTab === 'connections' ? <ConnectionsSettingsPanel /> : null}

            {activeTab === 'interface' ? <InterfaceSettingsPanel /> : null}

            {activeTab === 'tools' ? <ToolsSettingsPanel /> : null}

            {activeTab === 'security' ? (
              <SecuritySettingsPanel
                desktopApi={desktopApi}
                focusBackupPasswordRequest={securityFocusRequest}
                notice={securityNotice}
                onBackupPasswordFocusHandled={handleSecurityBackupPasswordFocusHandled}
                onBackupPasswordSaved={() => setSecurityNotice(null)}
              />
            ) : null}

            {activeTab === 'sync' ? <SyncSettingsPanel /> : null}

            {activeTab === 'updates' ? <UpdatesSettingsPanel /> : null}

            {activeTab === 'system' ? <SystemSettingsPanel /> : null}

            {activeTab === 'language' ? <LanguageSettingsPanel /> : null}
          </main>
        </div>
      </div>
    </SettingsModalProvider>
  )

  if (inline) {
    return content
  }

  if (standalone) {
    return <div className="manager-window">{content}</div>
  }

  return (
    <div className="modal-backdrop" onClick={syncOperation ? undefined : onClose}>
      {content}
    </div>
  )
}
