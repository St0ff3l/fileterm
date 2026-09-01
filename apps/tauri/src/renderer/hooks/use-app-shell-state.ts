import { useCallback, useEffect, useRef, useState } from 'react'
import {
  createCodexThemeConfig,
  createDefaultThemeConfig,
  DEFAULT_OVERVIEW_SECTION_ORDER,
  DEFAULT_RESOURCE_MONITORING_METRICS,
  DEFAULT_RESOURCE_MONITORING_METRIC_ORDER,
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  normalizeThemeConfig,
  type ActionApprovalRequest,
  type ConnectionImportPlan,
  type FileTermDesktopApi,
  type OverviewSectionId,
  type ResourceMonitoringMetric,
  type SavedTheme,
  type SshConnectionDefaults,
  type ThemeConfig,
  type UiPreferences
} from '@fileterm/core'
import { deriveThemeVariant, normalizeSavedTheme } from '../app/theme-config'
import { registerImportedFonts } from '../app/imported-fonts'
import { formatAppError, reportError, type ErrorDetails } from '../app/app-error-utils'
import { DEFAULT_SIDEBAR_WIDTH, FILE_PANEL_PREFERENCES_KEY, MAX_FILE_PANEL_RATIO } from '../app/app-shell-utils'
import { defaultLocale, setLocale, type AppLocale } from '../i18n'
import { resolveRendererPlatform } from '../lib/renderer-platform'
import { useWorkspaceIpcSync } from './use-workspace-ipc-sync'
import { useSessionSecurity } from './use-session-security'
import { useThemeMode, type ThemeMode } from './use-theme-mode'
import {
  DEFAULT_FILE_PANEL_SNAP_TARGET,
  isFilePanelSnapTarget,
  type FilePanelSnapTarget
} from '../features/workspace/file-panel-snap'

export type InitialUiPreferences = Pick<
  UiPreferences,
  | 'theme'
  | 'themeConfig'
  | 'customThemes'
  | 'locale'
  | 'connectionDefaults'
  | 'terminalZoomLocked'
  | 'filePanelRememberRatio'
  | 'resourceMonitoringMetrics'
  | 'resourceMonitoringMetricOrder'
  | 'overviewShowStats'
  | 'overviewShowRecent'
  | 'overviewShowAllConnections'
  | 'overviewShowQuickActions'
  | 'overviewSectionOrder'
>

export type AppShellOptions = {
  searchParams: URLSearchParams
  initialUiPreferences?: InitialUiPreferences
  desktopApi?: FileTermDesktopApi
  isConnectionFormWindow: boolean
  isMainWorkspaceWindow: boolean
  isConnectionManagerWindow: boolean
  rendererPlatform: ReturnType<typeof resolveRendererPlatform>
}

function readInitialTheme(searchParams: URLSearchParams, persistedPreferences?: InitialUiPreferences): ThemeMode {
  const queryTheme = searchParams.get('theme')
  if (queryTheme === 'default-light' || queryTheme === 'default-dark') {
    return queryTheme
  }
  if (persistedPreferences?.theme === 'default-light' || persistedPreferences?.theme === 'default-dark') {
    return persistedPreferences.theme
  }
  return 'default-dark'
}

function readInitialLocale(searchParams: URLSearchParams, persistedPreferences?: InitialUiPreferences): AppLocale {
  const queryLocale = searchParams.get('locale')
  if (queryLocale === 'enUS' || queryLocale === 'zhCN') {
    return queryLocale
  }
  if (persistedPreferences?.locale === 'enUS' || persistedPreferences?.locale === 'zhCN') {
    return persistedPreferences.locale
  }
  return defaultLocale
}

function sameOverviewSectionOrder(left: OverviewSectionId[], right: OverviewSectionId[]) {
  return left.length === right.length && left.every((sectionId, index) => sectionId === right[index])
}

function sameResourceMonitoringMetricOrder(left: ResourceMonitoringMetric[], right: ResourceMonitoringMetric[]) {
  return left.length === right.length && left.every((metric, index) => metric === right[index])
}

export function useAppShellState({
  searchParams,
  initialUiPreferences,
  desktopApi,
  isConnectionFormWindow,
  isMainWorkspaceWindow,
  isConnectionManagerWindow,
  rendererPlatform
}: AppShellOptions) {
  const [error, setError] = useState<string | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => readInitialTheme(searchParams, initialUiPreferences))
  const [themeConfig, setThemeConfig] = useState<ThemeConfig>(() => {
    const initialTheme = readInitialTheme(searchParams, initialUiPreferences)
    const variant = initialTheme === 'default-light' ? 'light' : 'dark'
    return initialUiPreferences?.themeConfig
      ? normalizeThemeConfig(initialUiPreferences.themeConfig, variant)
      : createDefaultThemeConfig(variant)
  })
  const [customThemes, setCustomThemes] = useState<SavedTheme[]>(() =>
    (initialUiPreferences?.customThemes ?? []).map(normalizeSavedTheme)
  )
  const [locale, setLocaleState] = useState<AppLocale>(() => readInitialLocale(searchParams, initialUiPreferences))
  const [connectionDefaults, setConnectionDefaults] = useState<SshConnectionDefaults>(() => ({
    ...DEFAULT_SSH_CONNECTION_DEFAULTS,
    ...(initialUiPreferences?.connectionDefaults ?? {})
  }))
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(() => initialUiPreferences?.terminalZoomLocked ?? false)
  const [filePanelRememberRatio, setFilePanelRememberRatio] = useState(
    () => initialUiPreferences?.filePanelRememberRatio ?? true
  )
  const [resourceMonitoringMetrics, setResourceMonitoringMetrics] = useState<ResourceMonitoringMetric[]>(() => [
    ...(initialUiPreferences?.resourceMonitoringMetrics ?? DEFAULT_RESOURCE_MONITORING_METRICS)
  ])
  const [resourceMonitoringMetricOrder, setResourceMonitoringMetricOrder] = useState<ResourceMonitoringMetric[]>(() => [
    ...(initialUiPreferences?.resourceMonitoringMetricOrder ?? DEFAULT_RESOURCE_MONITORING_METRIC_ORDER)
  ])
  const [overviewShowStats, setOverviewShowStats] = useState(() => initialUiPreferences?.overviewShowStats ?? true)
  const [overviewShowRecent, setOverviewShowRecent] = useState(() => initialUiPreferences?.overviewShowRecent ?? true)
  const [overviewShowAllConnections, setOverviewShowAllConnections] = useState(
    () => initialUiPreferences?.overviewShowAllConnections ?? true
  )
  const [overviewShowQuickActions, setOverviewShowQuickActions] = useState(
    () => initialUiPreferences?.overviewShowQuickActions ?? true
  )
  const [overviewSectionOrder, setOverviewSectionOrder] = useState<OverviewSectionId[]>(() => [
    ...(initialUiPreferences?.overviewSectionOrder ?? DEFAULT_OVERVIEW_SECTION_ORDER)
  ])

  const [connectionImportPlan, setConnectionImportPlan] = useState<ConnectionImportPlan | null>(null)
  const [actionApprovalRequests, setActionApprovalRequests] = useState<ActionApprovalRequest[]>([])
  const [resolvingActionApprovalId, setResolvingActionApprovalId] = useState<string | null>(null)
  const [riskAcknowledgedRequestId, setRiskAcknowledgedRequestId] = useState<string | null>(null)
  const resolvingActionApprovalIdsRef = useRef(new Set<string>())

  const [sidebarWidth, setSidebarWidth] = useState(DEFAULT_SIDEBAR_WIDTH)
  const [aiCopilotWidth, setAiCopilotWidth] = useState(368)
  const [filePanelHeights, setFilePanelHeights] = useState<Record<string, number>>({})
  const [filePanelRatios, setFilePanelRatios] = useState<Record<string, number>>({})
  const [filePanelSnapTargets, setFilePanelSnapTargets] = useState<Record<string, FilePanelSnapTarget | null>>({})
  const [hasLoadedFilePanelRatios, setHasLoadedFilePanelRatios] = useState(false)
  const [filePanelRatioPersistenceReady, setFilePanelRatioPersistenceReady] = useState(false)
  const [commandPaneWidths, setCommandPaneWidths] = useState<Record<string, number>>({})
  const [workspaceFocusModes, setWorkspaceFocusModes] = useState<Record<string, boolean>>({})
  const [workspaceViews, setWorkspaceViews] = useState<Record<string, 'file' | 'command' | 'tunnel'>>({})
  const [isResizingSidebar, setIsResizingSidebar] = useState(false)
  const [isResizingAiCopilot, setIsResizingAiCopilot] = useState(false)
  const [isAiCopilotOpen, setIsAiCopilotOpen] = useState(false)
  const [settingsInitialTab, setSettingsInitialTab] = useState<'interface' | 'ai'>('interface')
  const hasRevealedStandaloneWindowRef = useRef(false)

  const formatError = (scope: string, err: unknown, details?: ErrorDetails) =>
    formatAppError(scope, err, locale, details)
  const reportAppError = (setter: (message: string) => void, scope: string, err: unknown, details?: ErrorDetails) =>
    reportError(setter, locale, scope, err, details)
  const handleError = (scope: string, err: unknown, details?: ErrorDetails) =>
    reportAppError(setError, scope, err, details)

  const sessionSecurity = useSessionSecurity(desktopApi, isMainWorkspaceWindow)

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
        if (!canceled) {
          registerImportedFonts(
            entries.filter((entry): entry is { font: (typeof fonts)[number]; dataUrl: string } => entry !== null)
          )
        }
      })
      .catch((cause: unknown) => {
        if (!canceled) reportAppError(setError, '加载导入字体', cause)
      })

    return () => {
      canceled = true
    }
  }, [desktopApi])

  const openConnectionImportPreview = (source: 'files' | 'folder' = 'files') => {
    void desktopApi
      ?.previewConnectionImport(source)
      .then((plan) => plan && setConnectionImportPlan(plan))
      .catch((cause) => reportAppError(setError, '读取连接配置', cause))
  }

  const commitConnectionJsonPreview = async (
    selectedItemIds: string[],
    conflictStrategy: 'skip' | 'overwrite' | 'create'
  ) => {
    if (!connectionImportPlan || !desktopApi) return
    try {
      const result = await desktopApi.commitConnectionJsonImport(connectionImportPlan.id, {
        selectedItemIds,
        conflictStrategy
      })
      setConnectionImportPlan(null)
      setError(
        `连接导入：新增 ${result.imported}，覆盖 ${result.overwritten ?? 0}，跳过 ${result.skipped}，失败 ${result.failed}`
      )
    } catch (cause) {
      reportAppError(setError, '导入连接', cause)
    }
  }

  useThemeMode(themeMode, themeConfig)

  const handleSetTheme = useCallback((nextTheme: ThemeMode) => {
    setThemeMode(nextTheme)
    setThemeConfig((current) => {
      const nextVariant = nextTheme === 'default-light' ? 'light' : 'dark'
      if (current.variant === nextVariant) return current
      if (current.codeThemeId === 'codex' || current.codeThemeId.startsWith('codex-')) {
        return createCodexThemeConfig(nextVariant)
      }
      if (
        current.codeThemeId === 'fileterm' ||
        current.codeThemeId === 'fileterm-dark' ||
        current.codeThemeId === 'fileterm-light'
      ) {
        return createDefaultThemeConfig(nextVariant)
      }
      return deriveThemeVariant(current, nextVariant)
    })
  }, [])

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow) {
      return
    }

    return desktopApi.onActionApprovalRequest((request) => {
      // Copilot renders its approval inline beside the streamed tool call.
      // Keep the global dialog for MCP and Copilot tool approvals only.
      if (request.source === 'ai-copilot') {
        return
      }
      setActionApprovalRequests((current) => {
        if (current.some((item) => item.requestId === request.requestId)) {
          return current
        }
        return [...current, request]
      })
    })
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    const requestId = actionApprovalRequests[0]?.requestId ?? null
    setRiskAcknowledgedRequestId((current) => (current === requestId ? current : null))
  }, [actionApprovalRequests])

  const resolveActionApproval = useCallback(
    async (approved: boolean) => {
      const request = actionApprovalRequests[0]
      if (!desktopApi || !request || resolvingActionApprovalIdsRef.current.has(request.requestId)) {
        return
      }
      if (approved && request.requiresRiskAcknowledgement && riskAcknowledgedRequestId !== request.requestId) {
        return
      }

      resolvingActionApprovalIdsRef.current.add(request.requestId)
      setResolvingActionApprovalId(request.requestId)
      try {
        await desktopApi.resolveActionApproval(request.requestId, approved)
        setActionApprovalRequests((current) => current.filter((item) => item.requestId !== request.requestId))
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause))
      } finally {
        resolvingActionApprovalIdsRef.current.delete(request.requestId)
        setResolvingActionApprovalId((current) => (current === request.requestId ? null : current))
      }
    },
    [actionApprovalRequests, desktopApi, riskAcknowledgedRequestId]
  )

  const ipc = useWorkspaceIpcSync({
    desktopApi,
    isConnectionFormWindow,
    isMainWorkspaceWindow,
    isConnectionManagerWindow,
    themeMode,
    themeConfig,
    customThemes,
    locale,
    connectionDefaults,
    terminalZoomLocked,
    filePanelRememberRatio,
    resourceMonitoringMetrics,
    resourceMonitoringMetricOrder,
    overviewShowStats,
    overviewShowRecent,
    overviewShowAllConnections,
    overviewShowQuickActions,
    overviewSectionOrder,
    initialUiPreferencesLoaded: initialUiPreferences !== undefined,
    onThemeModeChange: setThemeMode,
    onThemeConfigChange: setThemeConfig,
    onCustomThemesChange: (nextThemes) => setCustomThemes(nextThemes.map(normalizeSavedTheme)),
    onLocaleChange: (nextLocale) => {
      setLocale(nextLocale)
      setLocaleState(nextLocale)
    },
    onConnectionDefaultsChange: (nextDefaults) => {
      setConnectionDefaults((currentDefaults) => ({ ...currentDefaults, ...nextDefaults }))
    },
    onTerminalZoomLockedChange: setTerminalZoomLocked,
    onFilePanelRememberRatioChange: setFilePanelRememberRatio,
    onResourceMonitoringMetricsChange: setResourceMonitoringMetrics,
    onResourceMonitoringMetricOrderChange: (nextOrder) => {
      setResourceMonitoringMetricOrder((currentOrder) =>
        sameResourceMonitoringMetricOrder(currentOrder, nextOrder) ? currentOrder : nextOrder
      )
    },
    onOverviewShowStatsChange: setOverviewShowStats,
    onOverviewShowRecentChange: setOverviewShowRecent,
    onOverviewShowAllConnectionsChange: setOverviewShowAllConnections,
    onOverviewShowQuickActionsChange: setOverviewShowQuickActions,
    onOverviewSectionOrderChange: (nextOrder) => {
      // Tauri returns a fresh array for every preference event. Keep the same
      // reference for an equal order so the persistence effect does not echo
      // the event back into an IPC update loop.
      setOverviewSectionOrder((currentOrder) =>
        sameOverviewSectionOrder(currentOrder, nextOrder) ? currentOrder : nextOrder
      )
    },
    onError: (scope, err) => reportAppError(setError, scope, err),
    onStatusMessage: (message) => setError(message)
  })

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow) {
      setHasLoadedFilePanelRatios(true)
      return
    }

    let canceled = false
    void desktopApi
      .getUiStateItem(FILE_PANEL_PREFERENCES_KEY)
      .then((value) => {
        if (canceled) return
        if (!value) {
          setFilePanelRatios({})
          setFilePanelSnapTargets({})
          setFilePanelRatioPersistenceReady(true)
          return
        }
        try {
          const parsed: unknown = JSON.parse(value)
          if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
            setFilePanelRatios({})
            setFilePanelSnapTargets({})
          } else {
            const normalizedRatios: Record<string, number> = {}
            const normalizedSnapTargets: Record<string, FilePanelSnapTarget | null> = {}
            Object.entries(parsed).forEach(([profileId, entry]) => {
              if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return
              const ratio = (entry as { ratio?: unknown }).ratio
              if (typeof ratio !== 'number' || !Number.isFinite(ratio)) return
              normalizedRatios[profileId] = Math.max(0, Math.min(MAX_FILE_PANEL_RATIO, ratio))
              const anchor = (entry as { anchor?: unknown }).anchor
              if (isFilePanelSnapTarget(anchor)) {
                normalizedSnapTargets[profileId] = anchor
              } else if (typeof anchor === 'boolean') {
                // Keep the v1 boolean format readable. `true` represented
                // the only target that existed at the time: the disk header.
                normalizedSnapTargets[profileId] = anchor ? DEFAULT_FILE_PANEL_SNAP_TARGET : null
              }
            })
            setFilePanelRatios(normalizedRatios)
            setFilePanelSnapTargets(normalizedSnapTargets)
          }
        } catch {
          setFilePanelRatios({})
          setFilePanelSnapTargets({})
        }
        setFilePanelRatioPersistenceReady(true)
      })
      .catch((cause: unknown) => reportAppError(setError, '读取文件面板布局', cause))
      .finally(() => {
        if (!canceled) setHasLoadedFilePanelRatios(true)
      })

    return () => {
      canceled = true
    }
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow || !hasLoadedFilePanelRatios || !filePanelRatioPersistenceReady) return
    const value = JSON.stringify(
      Object.fromEntries(
        Object.entries(filePanelRatios).map(([profileId, ratio]) => [
          profileId,
          {
            ratio,
            ...(filePanelSnapTargets[profileId] ? { anchor: filePanelSnapTargets[profileId] } : {})
          }
        ])
      )
    )
    void desktopApi.setUiStateItem(FILE_PANEL_PREFERENCES_KEY, value).catch((cause: unknown) => {
      reportAppError(setError, '保存文件面板布局', cause)
    })
  }, [
    desktopApi,
    filePanelRatios,
    filePanelSnapTargets,
    filePanelRatioPersistenceReady,
    hasLoadedFilePanelRatios,
    isMainWorkspaceWindow
  ])

  // Child windows and the transparent Linux main window remain hidden until
  // their route's first data fetch has settled.
  useEffect(() => {
    const waitsForFirstPaint = !isMainWorkspaceWindow || rendererPlatform === 'linux'
    if (!waitsForFirstPaint || !desktopApi || !ipc.hasLoadedInitialSnapshot || hasRevealedStandaloneWindowRef.current) {
      return
    }
    hasRevealedStandaloneWindowRef.current = true
    void desktopApi.showCurrentWindow().catch((cause) => reportAppError(setError, '显示窗口', cause))
  }, [desktopApi, ipc.hasLoadedInitialSnapshot, isMainWorkspaceWindow, rendererPlatform])

  useEffect(() => {
    if (rendererPlatform !== 'linux') {
      return
    }
    document.documentElement.dataset.windowMaximized = String(ipc.isMaximized)
    return () => {
      delete document.documentElement.dataset.windowMaximized
    }
  }, [ipc.isMaximized, rendererPlatform])

  return {
    ...ipc,
    desktopApi,
    rendererPlatform,
    isWindowsDesktop: rendererPlatform === 'win32',
    error,
    setError,
    isBusy,
    setIsBusy,
    themeMode,
    setThemeMode,
    themeConfig,
    setThemeConfig,
    customThemes,
    setCustomThemes,
    locale,
    setLocaleState,
    connectionDefaults,
    setConnectionDefaults,
    terminalZoomLocked,
    setTerminalZoomLocked,
    filePanelRememberRatio,
    setFilePanelRememberRatio,
    resourceMonitoringMetrics,
    setResourceMonitoringMetrics,
    resourceMonitoringMetricOrder,
    setResourceMonitoringMetricOrder,
    overviewShowStats,
    setOverviewShowStats,
    overviewShowRecent,
    setOverviewShowRecent,
    overviewShowAllConnections,
    setOverviewShowAllConnections,
    overviewShowQuickActions,
    setOverviewShowQuickActions,
    overviewSectionOrder,
    setOverviewSectionOrder,
    connectionImportPlan,
    setConnectionImportPlan,
    openConnectionImportPreview,
    commitConnectionJsonPreview,
    actionApprovalRequests,
    resolvingActionApprovalId,
    riskAcknowledgedRequestId,
    setRiskAcknowledgedRequestId,
    resolveActionApproval,
    sessionSecurity,
    handleSetTheme,
    formatAppError: formatError,
    reportError: handleError,
    sidebarWidth,
    setSidebarWidth,
    aiCopilotWidth,
    setAiCopilotWidth,
    filePanelHeights,
    setFilePanelHeights,
    filePanelRatios,
    setFilePanelRatios,
    filePanelSnapTargets,
    setFilePanelSnapTargets,
    hasLoadedFilePanelRatios,
    filePanelRatioPersistenceReady,
    commandPaneWidths,
    setCommandPaneWidths,
    workspaceFocusModes,
    setWorkspaceFocusModes,
    workspaceViews,
    setWorkspaceViews,
    isResizingSidebar,
    setIsResizingSidebar,
    isResizingAiCopilot,
    setIsResizingAiCopilot,
    isAiCopilotOpen,
    setIsAiCopilotOpen,
    settingsInitialTab,
    setSettingsInitialTab
  }
}
