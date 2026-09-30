import type { Dispatch, SetStateAction } from 'react'
import type {
  FileTermDesktopApi,
  LocalFileItem,
  OverviewSectionId,
  PaneFocusDirection,
  ResourceMonitoringMetric,
  SavedTheme,
  ThemeConfig,
  WorkspaceSnapshot
} from '@fileterm/core'
import type { AppLocale } from '../i18n'
import type { ThemeMode } from './use-theme-mode'
import type { SshConnectionDefaults } from './workspace-ipc-sync-utils'

export type WorkspaceWindowCloseRequest = {
  id: number
  isQuit: boolean
}

export type WorkspaceSplitPaneRequest = {
  id: number
  direction: 'row' | 'column'
}

export type WorkspacePaneFocusRequest = {
  id: number
  direction: PaneFocusDirection
}

export type UseWorkspaceIpcSyncOptions = {
  desktopApi?: FileTermDesktopApi
  isConnectionFormWindow: boolean
  isMainWorkspaceWindow: boolean
  isConnectionManagerWindow: boolean
  themeMode: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  locale: AppLocale
  connectionDefaults: SshConnectionDefaults
  terminalZoomLocked: boolean
  uiZoomLocked: boolean
  uiZoomPercent: number
  rememberWindowSize: boolean
  filePanelRememberRatio: boolean
  resourceMonitoringMetrics: ResourceMonitoringMetric[]
  resourceMonitoringMetricOrder: ResourceMonitoringMetric[]
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  initialUiPreferencesLoaded: boolean
  onThemeModeChange(themeMode: ThemeMode): void
  onThemeConfigChange(themeConfig: ThemeConfig): void
  onCustomThemesChange(customThemes: SavedTheme[]): void
  onLocaleChange(locale: AppLocale): void
  onConnectionDefaultsChange(value: Partial<SshConnectionDefaults>): void
  onTerminalZoomLockedChange(value: boolean): void
  onUiZoomLockedChange(value: boolean): void
  onUiZoomPercentChange(value: number): void
  onRememberWindowSizeChange(value: boolean): void
  onFilePanelRememberRatioChange(value: boolean): void
  onResourceMonitoringMetricsChange(value: ResourceMonitoringMetric[]): void
  onResourceMonitoringMetricOrderChange(value: ResourceMonitoringMetric[]): void
  onOverviewShowStatsChange(value: boolean): void
  onOverviewShowRecentChange(value: boolean): void
  onOverviewShowAllConnectionsChange(value: boolean): void
  onOverviewShowQuickActionsChange(value: boolean): void
  onOverviewSectionOrderChange(value: OverviewSectionId[]): void
  onError(scope: string, error: unknown): void
  onStatusMessage(message: string): void
}

export type UseWorkspaceIpcSyncResult = {
  workspace: WorkspaceSnapshot
  setWorkspace: Dispatch<SetStateAction<WorkspaceSnapshot>>
  applySnapshot(snapshot: WorkspaceSnapshot): boolean
  localPath: string
  setLocalPath: Dispatch<SetStateAction<string>>
  localItems: LocalFileItem[]
  setLocalItems: Dispatch<SetStateAction<LocalFileItem[]>>
  isLocalDirectoryLoading: boolean
  setIsLocalDirectoryLoading: Dispatch<SetStateAction<boolean>>
  hasLoadedInitialSnapshot: boolean
  isMaximized: boolean
  windowCloseRequest: WorkspaceWindowCloseRequest | null
  clearWindowCloseRequest(): void
  closeActiveRequestVersion: number
  newTabRequestVersion: number
  splitPaneRequest: WorkspaceSplitPaneRequest | null
  paneFocusRequest: WorkspacePaneFocusRequest | null
  closeCurrentWindow(): void
  requestQuitApp(): void
}
