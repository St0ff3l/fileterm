import { useRef, useState } from 'react'
import {
  DEFAULT_UI_ZOOM_PERCENT,
  DEFAULT_UI_FONT_SCALE_PERCENT,
  DEFAULT_MASK_CONNECTION_ADDRESSES,
  DEFAULT_LOCAL_TERMINAL_SHELLS,
  DEFAULT_MCP_AGENT_PREFERENCES,
  DEFAULT_OVERVIEW_SECTION_ORDER,
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  DEFAULT_TOPOLOGY_SERVICE_PROBE_INTERVAL_SECONDS,
  type AiProviderDraft,
  type AiProviderSummary,
  type AppUpdateStatus,
  type AppIconVariant,
  type BackupDownloadMode,
  type BackupUploadMode,
  type ConnectionProfile,
  type ImportedFont,
  type LocalTerminalShellOption,
  type LocalTerminalShellPreferences,
  type McpAgentPreferences,
  type McpAgentSetup,
  type OverviewSectionId,
  type S3BackupConfig,
  type SshConnectionDefaults,
  type TerminalRenderer,
  type UiPreferences,
  type WebDavSyncConfig
} from '@fileterm/core'
import type { ManagerDropPosition } from '../../../common/manager-drag'
import { getTerminalRendererPreference } from '../../../../app/terminal-renderer-preference'
import {
  createAiProviderDraft,
  type AiFeedback,
  type SettingsTab,
  type SyncFeedback as SettingsSyncFeedback
} from '../constants'

export function useSettingsModalState(initialTab: SettingsTab, uiPreferences?: UiPreferences) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab)
  const [settingsSearchQuery, setSettingsSearchQuery] = useState('')
  const [syncSubTab, setSyncSubTab] = useState<'webdav' | 's3'>('webdav')
  const [agentSubTab, setAgentSubTab] = useState<'mcp' | 'cli'>('mcp')
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(() => uiPreferences?.autoCheckUpdates ?? true)
  const [updateChannel, setUpdateChannel] = useState<'stable' | 'beta'>(() => uiPreferences?.updateChannel ?? 'beta')
  const [isSavingUpdatePreference, setIsSavingUpdatePreference] = useState(false)
  const [updatePreferenceError, setUpdatePreferenceError] = useState<string | null>(null)
  const [updatePreferenceLoadError, setUpdatePreferenceLoadError] = useState<string | null>(null)
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(() => uiPreferences?.terminalZoomLocked ?? false)
  const [terminalRenderer, setTerminalRenderer] = useState<TerminalRenderer>(
    () => uiPreferences?.terminalRenderer ?? getTerminalRendererPreference()
  )
  const [isSavingTerminalRendererPreference, setIsSavingTerminalRendererPreference] = useState(false)
  const [terminalRendererPreferenceError, setTerminalRendererPreferenceError] = useState<string | null>(null)
  const [isSavingTerminalZoomPreference, setIsSavingTerminalZoomPreference] = useState(false)
  const [terminalZoomPreferenceError, setTerminalZoomPreferenceError] = useState<string | null>(null)
  const [uiZoomLocked, setUiZoomLocked] = useState(() => uiPreferences?.uiZoomLocked ?? false)
  const [isSavingUiZoomPreference, setIsSavingUiZoomPreference] = useState(false)
  const [uiZoomPreferenceError, setUiZoomPreferenceError] = useState<string | null>(null)
  const [localTerminalShells, setLocalTerminalShells] = useState<LocalTerminalShellPreferences>(() => ({
    ...DEFAULT_LOCAL_TERMINAL_SHELLS,
    ...uiPreferences?.localTerminalShells
  }))
  const [localTerminalShellDrafts, setLocalTerminalShellDrafts] = useState<LocalTerminalShellPreferences>(() => ({
    ...DEFAULT_LOCAL_TERMINAL_SHELLS,
    ...uiPreferences?.localTerminalShells
  }))
  const [localTerminalShellOptions, setLocalTerminalShellOptions] = useState<LocalTerminalShellOption[]>([])
  const [isLoadingLocalTerminalShellOptions, setIsLoadingLocalTerminalShellOptions] = useState(false)
  const [localTerminalShellScanVersion, setLocalTerminalShellScanVersion] = useState(0)
  const [isSavingLocalTerminalShells, setIsSavingLocalTerminalShells] = useState(false)
  const [localTerminalShellMessage, setLocalTerminalShellMessage] = useState<string | null>(null)
  const [localTerminalShellError, setLocalTerminalShellError] = useState<string | null>(null)
  const [filePanelRememberRatio, setFilePanelRememberRatio] = useState(
    () => uiPreferences?.filePanelRememberRatio ?? true
  )
  const [isSavingFilePanelPreference, setIsSavingFilePanelPreference] = useState(false)
  const [filePanelPreferenceError, setFilePanelPreferenceError] = useState<string | null>(null)
  const [rememberWindowSize, setRememberWindowSize] = useState(() => uiPreferences?.rememberWindowSize ?? false)
  const [isSavingWindowSizePreference, setIsSavingWindowSizePreference] = useState(false)
  const [windowSizePreferenceError, setWindowSizePreferenceError] = useState<string | null>(null)
  const [showMacosMenuBarIcon, setShowMacosMenuBarIcon] = useState(() => uiPreferences?.showMacosMenuBarIcon ?? true)
  const [isSavingMacosMenuBarIcon, setIsSavingMacosMenuBarIcon] = useState(false)
  const [macosMenuBarIconPreferenceError, setMacosMenuBarIconPreferenceError] = useState<string | null>(null)
  const [appIcon, setAppIcon] = useState<AppIconVariant>(() => uiPreferences?.appIcon ?? 'classic')
  const [isSavingAppIcon, setIsSavingAppIcon] = useState(false)
  const [appIconPreferenceError, setAppIconPreferenceError] = useState<string | null>(null)
  const [uiZoomPercent, setUiZoomPercent] = useState(() => uiPreferences?.uiZoomPercent ?? DEFAULT_UI_ZOOM_PERCENT)
  const [isSavingUiZoomPercent, setIsSavingUiZoomPercent] = useState(false)
  const [uiZoomPercentError, setUiZoomPercentError] = useState<string | null>(null)
  const [uiFontScalePercent, setUiFontScalePercent] = useState(
    () => uiPreferences?.uiFontScalePercent ?? DEFAULT_UI_FONT_SCALE_PERCENT
  )
  const [isSavingUiFontScalePercent, setIsSavingUiFontScalePercent] = useState(false)
  const [uiFontScalePercentError, setUiFontScalePercentError] = useState<string | null>(null)
  const [maskConnectionAddresses, setMaskConnectionAddresses] = useState(
    () => uiPreferences?.maskConnectionAddresses ?? DEFAULT_MASK_CONNECTION_ADDRESSES
  )
  const [topologyServiceProbeIntervalSeconds, setTopologyServiceProbeIntervalSeconds] = useState(
    () => uiPreferences?.topologyServiceProbeIntervalSeconds ?? DEFAULT_TOPOLOGY_SERVICE_PROBE_INTERVAL_SECONDS
  )
  const [isSavingTopologyServiceProbeInterval, setIsSavingTopologyServiceProbeInterval] = useState(false)
  const [topologyServiceProbeIntervalError, setTopologyServiceProbeIntervalError] = useState<string | null>(null)
  const [isSavingMaskConnectionAddresses, setIsSavingMaskConnectionAddresses] = useState(false)
  const [maskConnectionAddressesError, setMaskConnectionAddressesError] = useState<string | null>(null)
  const [importedFonts, setImportedFonts] = useState<ImportedFont[]>([])
  const [isImportingFont, setIsImportingFont] = useState(false)
  const [fontImportError, setFontImportError] = useState<string | null>(null)
  const [fontImportLoadError, setFontImportLoadError] = useState<string | null>(null)
  const [fontToDelete, setFontToDelete] = useState<ImportedFont | null>(null)
  const [themeConfigMessage, setThemeConfigMessage] = useState<{
    id: number
    text: string
    kind: 'success' | 'error' | 'warning'
  } | null>(null)
  const [customThemeName, setCustomThemeName] = useState('')
  const [editingCustomThemeId, setEditingCustomThemeId] = useState<string | null>(null)
  const [showDeleteThemeConfirm, setShowDeleteThemeConfirm] = useState(false)
  const [mcpAgentPreferences, setMcpAgentPreferences] = useState<McpAgentPreferences>(() => ({
    ...DEFAULT_MCP_AGENT_PREFERENCES,
    ...uiPreferences?.mcpAgent
  }))
  const [mcpAgentSetup, setMcpAgentSetup] = useState<McpAgentSetup | null>(null)
  const [mcpAgentProfiles, setMcpAgentProfiles] = useState<ConnectionProfile[]>([])
  const [mcpAgentProfileSearch, setMcpAgentProfileSearch] = useState('')
  const [mcpAgentOperation, setMcpAgentOperation] = useState<'load' | 'save' | null>(null)
  const [mcpAgentMessage, setMcpAgentMessage] = useState<string | null>(null)
  const [connectionDefaults, setConnectionDefaults] = useState<SshConnectionDefaults>(() => ({
    ...DEFAULT_SSH_CONNECTION_DEFAULTS,
    ...uiPreferences?.connectionDefaults
  }))
  const [isSavingConnectionDefaults, setIsSavingConnectionDefaults] = useState(false)
  const [connectionDefaultsError, setConnectionDefaultsError] = useState<string | null>(null)
  const [transferSidebarCollapsedByDefault, setTransferSidebarCollapsedByDefault] = useState(
    () => uiPreferences?.transferSidebarCollapsedByDefault ?? true
  )
  const [isSavingTransferSidebarPreference, setIsSavingTransferSidebarPreference] = useState(false)
  const [transferSidebarPreferenceError, setTransferSidebarPreferenceError] = useState<string | null>(null)
  const [overviewShowStats, setOverviewShowStats] = useState(() => uiPreferences?.overviewShowStats ?? true)
  const [overviewShowRecent, setOverviewShowRecent] = useState(() => uiPreferences?.overviewShowRecent ?? true)
  const [overviewShowAllConnections, setOverviewShowAllConnections] = useState(
    () => uiPreferences?.overviewShowAllConnections ?? true
  )
  const [overviewShowQuickActions, setOverviewShowQuickActions] = useState(
    () => uiPreferences?.overviewShowQuickActions ?? true
  )
  const [overviewSectionOrder, setOverviewSectionOrder] = useState<OverviewSectionId[]>(() => [
    ...(uiPreferences?.overviewSectionOrder ?? DEFAULT_OVERVIEW_SECTION_ORDER)
  ])
  const [draggingOverviewSection, setDraggingOverviewSection] = useState<OverviewSectionId | null>(null)
  const [dragOverOverviewSection, setDragOverOverviewSection] = useState<OverviewSectionId | null>(null)
  const [overviewDragPosition, setOverviewDragPosition] = useState<ManagerDropPosition | null>(null)
  const [isSavingOverviewPreference, setIsSavingOverviewPreference] = useState(false)
  const [overviewPreferenceError, setOverviewPreferenceError] = useState<string | null>(null)
  const [syncConfig, setSyncConfig] = useState<WebDavSyncConfig | null>(null)
  const [syncPassword, setSyncPassword] = useState('')
  const [syncFeedback, setSyncFeedback] = useState<SettingsSyncFeedback | null>(null)
  const [securityNotice, setSecurityNotice] = useState<string | null>(null)
  const [securityFocusRequest, setSecurityFocusRequest] = useState(0)
  const [s3Config, setS3Config] = useState<S3BackupConfig | null>(null)
  const [s3SecretAccessKey, setS3SecretAccessKey] = useState('')
  const [s3Feedback, setS3Feedback] = useState<SettingsSyncFeedback | null>(null)
  const [backupUploadMode, setBackupUploadMode] = useState<BackupUploadMode>('overwrite-cloud')
  const [backupDownloadMode, setBackupDownloadMode] = useState<BackupDownloadMode>('merge-local')
  const [aiProviders, setAiProviders] = useState<AiProviderSummary[]>([])
  const [aiDraft, setAiDraft] = useState<AiProviderDraft>(() => createAiProviderDraft())
  const [aiModelChoices, setAiModelChoices] = useState<string[]>([])
  const [configuredModels, setConfiguredModels] = useState<string[]>([])
  const [selectedCandidateModel, setSelectedCandidateModel] = useState<string>('')
  const [isCustomInput, setIsCustomInput] = useState(false)
  const [customModelText, setCustomModelText] = useState('')
  const [aiApiKey, setAiApiKey] = useState('')
  const [clearAiApiKey, setClearAiApiKey] = useState(false)
  const [aiMessage, setAiMessage] = useState<AiFeedback | null>(null)
  const [aiOperation, setAiOperation] = useState<'load' | 'models' | 'save' | 'test' | 'delete' | null>(null)
  const aiActionInFlightRef = useRef(false)
  const [showDeleteAiProviderConfirm, setShowDeleteAiProviderConfirm] = useState(false)
  const [syncOperation, setSyncOperation] = useState<
    'load' | 'save' | 'test' | 'upload' | 'download' | 's3-save' | 's3-test' | 's3-upload' | 's3-download' | null
  >(null)
  const syncOperationRef = useRef<typeof syncOperation>(null)
  const overviewDragStateRef = useRef<{
    source: OverviewSectionId | null
    target: OverviewSectionId | null
    position: ManagerDropPosition | null
  }>({ source: null, target: null, position: null })
  const suppressOverviewCardClickRef = useRef(false)

  return {
    activeTab,
    setActiveTab,
    settingsSearchQuery,
    setSettingsSearchQuery,
    syncSubTab,
    setSyncSubTab,
    agentSubTab,
    setAgentSubTab,
    updateStatus,
    setUpdateStatus,
    autoCheckUpdates,
    setAutoCheckUpdates,
    updateChannel,
    setUpdateChannel,
    isSavingUpdatePreference,
    setIsSavingUpdatePreference,
    updatePreferenceError,
    setUpdatePreferenceError,
    updatePreferenceLoadError,
    setUpdatePreferenceLoadError,
    terminalZoomLocked,
    setTerminalZoomLocked,
    terminalRenderer,
    setTerminalRenderer,
    isSavingTerminalRendererPreference,
    setIsSavingTerminalRendererPreference,
    terminalRendererPreferenceError,
    setTerminalRendererPreferenceError,
    isSavingTerminalZoomPreference,
    setIsSavingTerminalZoomPreference,
    terminalZoomPreferenceError,
    setTerminalZoomPreferenceError,
    uiZoomLocked,
    setUiZoomLocked,
    isSavingUiZoomPreference,
    setIsSavingUiZoomPreference,
    uiZoomPreferenceError,
    setUiZoomPreferenceError,
    localTerminalShells,
    setLocalTerminalShells,
    localTerminalShellDrafts,
    setLocalTerminalShellDrafts,
    localTerminalShellOptions,
    setLocalTerminalShellOptions,
    isLoadingLocalTerminalShellOptions,
    setIsLoadingLocalTerminalShellOptions,
    localTerminalShellScanVersion,
    setLocalTerminalShellScanVersion,
    isSavingLocalTerminalShells,
    setIsSavingLocalTerminalShells,
    localTerminalShellMessage,
    setLocalTerminalShellMessage,
    localTerminalShellError,
    setLocalTerminalShellError,
    filePanelRememberRatio,
    setFilePanelRememberRatio,
    isSavingFilePanelPreference,
    setIsSavingFilePanelPreference,
    filePanelPreferenceError,
    setFilePanelPreferenceError,
    rememberWindowSize,
    setRememberWindowSize,
    isSavingWindowSizePreference,
    setIsSavingWindowSizePreference,
    windowSizePreferenceError,
    setWindowSizePreferenceError,
    showMacosMenuBarIcon,
    setShowMacosMenuBarIcon,
    isSavingMacosMenuBarIcon,
    setIsSavingMacosMenuBarIcon,
    macosMenuBarIconPreferenceError,
    setMacosMenuBarIconPreferenceError,
    appIcon,
    setAppIcon,
    isSavingAppIcon,
    setIsSavingAppIcon,
    appIconPreferenceError,
    setAppIconPreferenceError,
    uiZoomPercent,
    setUiZoomPercent,
    isSavingUiZoomPercent,
    setIsSavingUiZoomPercent,
    uiZoomPercentError,
    setUiZoomPercentError,
    uiFontScalePercent,
    setUiFontScalePercent,
    isSavingUiFontScalePercent,
    setIsSavingUiFontScalePercent,
    uiFontScalePercentError,
    setUiFontScalePercentError,
    maskConnectionAddresses,
    setMaskConnectionAddresses,
    topologyServiceProbeIntervalSeconds,
    setTopologyServiceProbeIntervalSeconds,
    isSavingTopologyServiceProbeInterval,
    setIsSavingTopologyServiceProbeInterval,
    topologyServiceProbeIntervalError,
    setTopologyServiceProbeIntervalError,
    isSavingMaskConnectionAddresses,
    setIsSavingMaskConnectionAddresses,
    maskConnectionAddressesError,
    setMaskConnectionAddressesError,
    importedFonts,
    setImportedFonts,
    isImportingFont,
    setIsImportingFont,
    fontImportError,
    setFontImportError,
    fontImportLoadError,
    setFontImportLoadError,
    fontToDelete,
    setFontToDelete,
    themeConfigMessage,
    setThemeConfigMessage,
    customThemeName,
    setCustomThemeName,
    editingCustomThemeId,
    setEditingCustomThemeId,
    showDeleteThemeConfirm,
    setShowDeleteThemeConfirm,
    mcpAgentPreferences,
    setMcpAgentPreferences,
    mcpAgentSetup,
    setMcpAgentSetup,
    mcpAgentProfiles,
    setMcpAgentProfiles,
    mcpAgentProfileSearch,
    setMcpAgentProfileSearch,
    mcpAgentOperation,
    setMcpAgentOperation,
    mcpAgentMessage,
    setMcpAgentMessage,
    connectionDefaults,
    setConnectionDefaults,
    isSavingConnectionDefaults,
    setIsSavingConnectionDefaults,
    connectionDefaultsError,
    setConnectionDefaultsError,
    transferSidebarCollapsedByDefault,
    setTransferSidebarCollapsedByDefault,
    isSavingTransferSidebarPreference,
    setIsSavingTransferSidebarPreference,
    transferSidebarPreferenceError,
    setTransferSidebarPreferenceError,
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
    draggingOverviewSection,
    setDraggingOverviewSection,
    dragOverOverviewSection,
    setDragOverOverviewSection,
    overviewDragPosition,
    setOverviewDragPosition,
    isSavingOverviewPreference,
    setIsSavingOverviewPreference,
    overviewPreferenceError,
    setOverviewPreferenceError,
    syncConfig,
    setSyncConfig,
    syncPassword,
    setSyncPassword,
    syncFeedback,
    setSyncFeedback,
    securityNotice,
    setSecurityNotice,
    securityFocusRequest,
    setSecurityFocusRequest,
    s3Config,
    setS3Config,
    s3SecretAccessKey,
    setS3SecretAccessKey,
    s3Feedback,
    setS3Feedback,
    backupUploadMode,
    setBackupUploadMode,
    backupDownloadMode,
    setBackupDownloadMode,
    aiProviders,
    setAiProviders,
    aiDraft,
    setAiDraft,
    aiModelChoices,
    setAiModelChoices,
    configuredModels,
    setConfiguredModels,
    selectedCandidateModel,
    setSelectedCandidateModel,
    isCustomInput,
    setIsCustomInput,
    customModelText,
    setCustomModelText,
    aiApiKey,
    setAiApiKey,
    clearAiApiKey,
    setClearAiApiKey,
    aiMessage,
    setAiMessage,
    aiOperation,
    setAiOperation,
    aiActionInFlightRef,
    showDeleteAiProviderConfirm,
    setShowDeleteAiProviderConfirm,
    syncOperation,
    setSyncOperation,
    syncOperationRef,
    overviewDragStateRef,
    suppressOverviewCardClickRef,
    desktopApi: window.fileterm,
    updatePreviewState: import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined
  }
}

export type SettingsModalState = ReturnType<typeof useSettingsModalState>

export type SettingsModalDesktopApi = SettingsModalState['desktopApi']
