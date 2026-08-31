import { useRef, useState } from 'react'
import {
  DEFAULT_LOCAL_TERMINAL_SHELLS,
  DEFAULT_MCP_AGENT_PREFERENCES,
  DEFAULT_OVERVIEW_SECTION_ORDER,
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  type AiProviderDraft,
  type AiProviderSummary,
  type AppUpdateStatus,
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
  type WebDavSyncConfig
} from '@fileterm/core'
import type { ManagerDropPosition } from '../../../common/manager-drag'
import {
  createAiProviderDraft,
  type AiFeedback,
  type SettingsTab,
  type SyncFeedback as SettingsSyncFeedback
} from '../constants'

export function useSettingsModalState(initialTab: SettingsTab) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab)
  const [settingsSearchQuery, setSettingsSearchQuery] = useState('')
  const [syncSubTab, setSyncSubTab] = useState<'webdav' | 's3'>('webdav')
  const [agentSubTab, setAgentSubTab] = useState<'mcp' | 'cli'>('mcp')
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(true)
  const [updateChannel, setUpdateChannel] = useState<'stable' | 'beta'>('stable')
  const [isSavingUpdatePreference, setIsSavingUpdatePreference] = useState(false)
  const [updatePreferenceError, setUpdatePreferenceError] = useState<string | null>(null)
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(false)
  const [isSavingTerminalZoomPreference, setIsSavingTerminalZoomPreference] = useState(false)
  const [terminalZoomPreferenceError, setTerminalZoomPreferenceError] = useState<string | null>(null)
  const [localTerminalShells, setLocalTerminalShells] = useState<LocalTerminalShellPreferences>(() => ({
    ...DEFAULT_LOCAL_TERMINAL_SHELLS
  }))
  const [localTerminalShellDrafts, setLocalTerminalShellDrafts] = useState<LocalTerminalShellPreferences>(() => ({
    ...DEFAULT_LOCAL_TERMINAL_SHELLS
  }))
  const [localTerminalShellOptions, setLocalTerminalShellOptions] = useState<LocalTerminalShellOption[]>([])
  const [isLoadingLocalTerminalShellOptions, setIsLoadingLocalTerminalShellOptions] = useState(false)
  const [localTerminalShellScanVersion, setLocalTerminalShellScanVersion] = useState(0)
  const [isSavingLocalTerminalShells, setIsSavingLocalTerminalShells] = useState(false)
  const [localTerminalShellMessage, setLocalTerminalShellMessage] = useState<string | null>(null)
  const [localTerminalShellError, setLocalTerminalShellError] = useState<string | null>(null)
  const [filePanelRememberRatio, setFilePanelRememberRatio] = useState(true)
  const [isSavingFilePanelPreference, setIsSavingFilePanelPreference] = useState(false)
  const [filePanelPreferenceError, setFilePanelPreferenceError] = useState<string | null>(null)
  const [importedFonts, setImportedFonts] = useState<ImportedFont[]>([])
  const [fontImportKind, setFontImportKind] = useState<'ui' | 'code' | null>(null)
  const [fontImportError, setFontImportError] = useState<string | null>(null)
  const [fontToDelete, setFontToDelete] = useState<ImportedFont | null>(null)
  const [themeConfigOperation, setThemeConfigOperation] = useState<'import' | 'copy' | null>(null)
  const themeConfigOperationRef = useRef<typeof themeConfigOperation>(null)
  const [themeConfigMessage, setThemeConfigMessage] = useState<{
    text: string
    kind: 'success' | 'error' | 'warning'
  } | null>(null)
  const [customThemeName, setCustomThemeName] = useState('')
  const [editingCustomThemeId, setEditingCustomThemeId] = useState<string | null>(null)
  const [showDeleteThemeConfirm, setShowDeleteThemeConfirm] = useState(false)
  const [mcpAgentPreferences, setMcpAgentPreferences] = useState<McpAgentPreferences>(() => ({
    ...DEFAULT_MCP_AGENT_PREFERENCES
  }))
  const [mcpAgentSetup, setMcpAgentSetup] = useState<McpAgentSetup | null>(null)
  const [mcpAgentProfiles, setMcpAgentProfiles] = useState<ConnectionProfile[]>([])
  const [mcpAgentProfileSearch, setMcpAgentProfileSearch] = useState('')
  const [mcpAgentOperation, setMcpAgentOperation] = useState<'load' | 'save' | null>(null)
  const [mcpAgentMessage, setMcpAgentMessage] = useState<string | null>(null)
  const [connectionDefaults, setConnectionDefaults] = useState<SshConnectionDefaults>(() => ({
    ...DEFAULT_SSH_CONNECTION_DEFAULTS
  }))
  const [isSavingConnectionDefaults, setIsSavingConnectionDefaults] = useState(false)
  const [connectionDefaultsError, setConnectionDefaultsError] = useState<string | null>(null)
  const [overviewShowStats, setOverviewShowStats] = useState(true)
  const [overviewShowRecent, setOverviewShowRecent] = useState(true)
  const [overviewShowAllConnections, setOverviewShowAllConnections] = useState(true)
  const [overviewShowQuickActions, setOverviewShowQuickActions] = useState(true)
  const [overviewSectionOrder, setOverviewSectionOrder] = useState<OverviewSectionId[]>(() => [
    ...DEFAULT_OVERVIEW_SECTION_ORDER
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
  const [aiOperation, setAiOperation] = useState<'load' | 'save' | 'test' | 'delete' | null>(null)
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
    terminalZoomLocked,
    setTerminalZoomLocked,
    isSavingTerminalZoomPreference,
    setIsSavingTerminalZoomPreference,
    terminalZoomPreferenceError,
    setTerminalZoomPreferenceError,
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
    importedFonts,
    setImportedFonts,
    fontImportKind,
    setFontImportKind,
    fontImportError,
    setFontImportError,
    fontToDelete,
    setFontToDelete,
    themeConfigOperation,
    setThemeConfigOperation,
    themeConfigOperationRef,
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
