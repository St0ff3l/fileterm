import type { McpAgentClientStatus, SavedTheme, ThemeConfig, UiPreferences, WorkspaceMode } from '@fileterm/core'
import type { ThemeMode } from '../../../../app/theme-config'
import type { SettingsTab } from '../constants'

export type SettingsModalControllerOptions = {
  theme: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  onSetTheme(value: ThemeMode): void
  onSetThemeConfig(value: ThemeConfig): void
  onSetCustomThemes(value: SavedTheme[]): void
  locale: 'zhCN' | 'enUS'
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  onLaunchLocalAgent?(client: McpAgentClientStatus): void
  workspaceMode: WorkspaceMode
  uiPreferences: UiPreferences | undefined
  initialTab: SettingsTab
}
