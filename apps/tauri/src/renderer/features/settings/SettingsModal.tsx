import { useEffect, useMemo, useRef, useState } from 'react'
import {
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  DEFAULT_MCP_AGENT_PREFERENCES,
  DEFAULT_OVERVIEW_SECTION_ORDER,
  createCodexThemeConfig,
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type AppUpdateStatus,
  type AiProviderDraft,
  type AiProviderKind,
  type AiProviderSummary,
  type BackupDownloadMode,
  type BackupUploadMode,
  type ConnectionProfile,
  type McpAgentClientStatus,
  type McpAgentPreferences,
  type McpAgentSetup,
  type OverviewSectionId,
  type S3BackupConfig,
  type SavedTheme,
  type SshConnectionDefaults,
  type TerminalAnsiColorName,
  type ThemeConfig,
  type UiPreferences,
  type WebDavSyncConfig
} from '@fileterm/core'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/usePointerSortFallback'
import { t, type LocaleMessages } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { DropdownSelect } from '../common/DropdownSelect'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from '../common/manager-drag'
import { targetsNestedManagerControl } from '../common/manager-interactions'

type SettingsTab = 'ai' | 'agent' | 'connections' | 'interface' | 'sync' | 'tools' | 'updates' | 'system' | 'language'

type ThemePresetFamily = 'fileterm' | 'codex'
type ThemePresetVariant = ThemeConfig['variant']

const THEME_HEX_COLOR_PATTERN = /^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i
const THEME_CONFIG_EXPORT_PREFIX = 'fileterm-theme-v1:'
const THEME_CONFIG_IMPORT_PREFIXES = [THEME_CONFIG_EXPORT_PREFIX, 'codex-theme-v1:'] as const

const ANSI_COLOR_NAMES: TerminalAnsiColorName[] = [
  'black',
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'white',
  'brightBlack',
  'brightRed',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
  'brightWhite'
]

const ANSI_COLOR_LABELS: Record<TerminalAnsiColorName, string> = {
  black: 'Black',
  red: 'Red',
  green: 'Green',
  yellow: 'Yellow',
  blue: 'Blue',
  magenta: 'Magenta',
  cyan: 'Cyan',
  white: 'White',
  brightBlack: 'Bright Black',
  brightRed: 'Bright Red',
  brightGreen: 'Bright Green',
  brightYellow: 'Bright Yellow',
  brightBlue: 'Bright Blue',
  brightMagenta: 'Bright Magenta',
  brightCyan: 'Bright Cyan',
  brightWhite: 'Bright White'
}

const THEME_PRESETS: Array<{
  id: ThemePresetFamily
  labelKey: 'themePresetFileTerm' | 'themePresetCodex'
  config: Record<ThemePresetVariant, ThemeConfig>
}> = [
  {
    id: 'fileterm',
    labelKey: 'themePresetFileTerm',
    config: { dark: createDefaultThemeConfig('dark'), light: createDefaultThemeConfig('light') }
  },
  {
    id: 'codex',
    labelKey: 'themePresetCodex',
    config: { dark: createCodexThemeConfig('dark'), light: createCodexThemeConfig('light') }
  }
]

function findMatchingThemePreset(themeConfig: ThemeConfig): (typeof THEME_PRESETS)[number] | undefined {
  return THEME_PRESETS.find((preset) => {
    const candidate = preset.config[themeConfig.variant]
    const matchesId =
      preset.id === 'fileterm'
        ? themeConfig.codeThemeId === 'fileterm' ||
          themeConfig.codeThemeId === 'fileterm-dark' ||
          themeConfig.codeThemeId === 'fileterm-light'
        : themeConfig.codeThemeId === 'codex' ||
          themeConfig.codeThemeId === 'codex-dark' ||
          themeConfig.codeThemeId === 'codex-light'
    if (!matchesId) return false
    const colorValues = [
      candidate.theme.accent,
      candidate.theme.surface,
      candidate.theme.surfaceSecondary,
      candidate.theme.surfaceElevated,
      candidate.theme.ink,
      candidate.theme.semanticColors.secondary,
      candidate.theme.semanticColors.textSecondary,
      candidate.theme.semanticColors.info,
      candidate.theme.semanticColors.warning,
      candidate.theme.semanticColors.error,
      candidate.theme.semanticColors.success
    ]
    const themeColorValues = [
      themeConfig.theme.accent,
      themeConfig.theme.surface,
      themeConfig.theme.surfaceSecondary,
      themeConfig.theme.surfaceElevated,
      themeConfig.theme.ink,
      themeConfig.theme.semanticColors.secondary,
      themeConfig.theme.semanticColors.textSecondary,
      themeConfig.theme.semanticColors.info,
      themeConfig.theme.semanticColors.warning,
      themeConfig.theme.semanticColors.error,
      themeConfig.theme.semanticColors.success
    ]
    return (
      colorValues.every((value, index) => value.toUpperCase() === themeColorValues[index].toUpperCase()) &&
      candidate.theme.contrast === themeConfig.theme.contrast &&
      candidate.theme.opaqueWindows === themeConfig.theme.opaqueWindows
    )
  })
}

function sameThemeConfig(left: ThemeConfig, right: ThemeConfig) {
  return JSON.stringify(left) === JSON.stringify(right)
}

function themeBaseIdForConfig(themeConfig: ThemeConfig): 'fileterm' | 'codex' {
  if (themeConfig.baseThemeId) return themeConfig.baseThemeId
  return themeConfig.codeThemeId === 'codex' || themeConfig.codeThemeId.startsWith('codex-') ? 'codex' : 'fileterm'
}

function createCustomThemeId() {
  const randomId = typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : null
  return `custom-${randomId ?? `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`}`
}

function toColorInputValue(value: string) {
  const normalized = value.trim()
  if (/^#[\da-f]{6}$/i.test(normalized)) return normalized
  if (/^#[\da-f]{8}$/i.test(normalized)) return normalized.slice(0, 7)
  if (/^#[\da-f]{3}$/i.test(normalized)) {
    return `#${normalized
      .slice(1)
      .split('')
      .map((part) => `${part}${part}`)
      .join('')}`
  }
  if (/^#[\da-f]{4}$/i.test(normalized)) {
    return `#${normalized
      .slice(1, 4)
      .split('')
      .map((part) => `${part}${part}`)
      .join('')}`
  }
  return '#000000'
}

function ThemeColorField({ label, value, onChange }: { label: string; value: string; onChange(value: string): void }) {
  const [draft, setDraft] = useState(value)

  useEffect(() => {
    setDraft(value)
  }, [value])

  return (
    <label className="theme-color-field">
      <span className="theme-color-field-label">{label}</span>
      <span className="theme-color-field-control">
        <input
          aria-label={label}
          className="theme-color-picker"
          onChange={(event) => onChange(event.target.value.toUpperCase())}
          type="color"
          value={toColorInputValue(value)}
        />
        <input
          aria-label={`${label} HEX`}
          className="theme-color-text"
          onBlur={() => setDraft(value)}
          onChange={(event) => {
            const nextValue = event.target.value
            setDraft(nextValue)
            if (THEME_HEX_COLOR_PATTERN.test(nextValue.trim())) {
              onChange(nextValue.trim().toUpperCase())
            }
          }}
          spellCheck={false}
          value={draft}
        />
      </span>
    </label>
  )
}

function clipboardUnavailableError(lastError: unknown) {
  return lastError instanceof Error ? lastError : new Error('Clipboard is unavailable')
}

function sameOverviewSectionOrder(left: OverviewSectionId[], right: OverviewSectionId[]) {
  return left.length === right.length && left.every((sectionId, index) => sectionId === right[index])
}

const DEFAULT_MODELS_BY_KIND: Record<AiProviderKind, string[]> = {
  'openai-compatible-chat': ['deepseek-v4-flash', 'deepseek-v4-pro', 'gpt-5.6-sol', 'kimi-k3', 'qwen-max'],
  'openai-responses': ['gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.5-pro', 'o3', 'o4-mini'],
  'anthropic-messages': ['claude-opus-5', 'claude-sonnet-5', 'claude-haiku-4.5']
}

function createAiProviderDraft(isDefault = true): AiProviderDraft {
  return {
    name: '',
    kind: 'openai-compatible-chat',
    baseUrl: '',
    model: '',
    enabled: true,
    isDefault,
    allowNoAuth: false,
    allowInsecureHttp: false
  }
}

type AiProviderPreset = {
  id: string
  // Stable translation key suffix, resolved via `t.aiSettingsPreset_<id>`.
  labelKey: keyof LocaleMessages
  draft: {
    name: string
    kind: AiProviderKind
    baseUrl: string
    model: string
    // When non-empty, the form renders a DropdownSelect letting the user pick
    // from this provider's latest model batch instead of typing an ID by hand.
    models?: string[]
    allowNoAuth: boolean
    allowInsecureHttp: boolean
  }
}

// Curated provider presets aligned with cc-switch's common defaults so users
// can fill the form with one click instead of looking up Base URL / model IDs.
const AI_PROVIDER_PRESETS: AiProviderPreset[] = [
  {
    id: 'anthropic-official',
    labelKey: 'aiSettingsPreset_anthropicOfficial',
    draft: {
      name: 'Anthropic',
      kind: 'anthropic-messages',
      baseUrl: 'https://api.anthropic.com/v1',
      model: 'claude-opus-5',
      models: ['claude-opus-5', 'claude-sonnet-5', 'claude-fable-5', 'claude-haiku-4.5'],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'openai-responses',
    labelKey: 'aiSettingsPreset_openaiResponses',
    draft: {
      name: 'OpenAI Responses',
      kind: 'openai-responses',
      baseUrl: 'https://api.openai.com/v1',
      model: 'gpt-5.6-sol',
      models: [
        'gpt-5.6-sol',
        'gpt-5.6-terra',
        'gpt-5.6-luna',
        'gpt-5.5-pro',
        'gpt-5.4-pro',
        'gpt-5.4-mini',
        'o3',
        'o4-mini'
      ],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'deepseek-chat',
    labelKey: 'aiSettingsPreset_deepseekChat',
    draft: {
      name: 'DeepSeek',
      kind: 'openai-compatible-chat',
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-flash',
      models: ['deepseek-v4-flash', 'deepseek-v4-pro'],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'kimi-moonshot',
    labelKey: 'aiSettingsPreset_kimiMoonshot',
    draft: {
      name: 'Kimi (Moonshot)',
      kind: 'openai-compatible-chat',
      baseUrl: 'https://api.moonshot.cn/v1',
      model: 'kimi-k3',
      models: ['kimi-k3', 'kimi-k2.7-code', 'kimi-k2.6'],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'glm-zhipu',
    labelKey: 'aiSettingsPreset_glmZhipu',
    draft: {
      name: 'GLM (智谱)',
      kind: 'openai-compatible-chat',
      baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
      model: 'glm-5.2',
      models: ['glm-5.2', 'glm-5.1', 'glm-5', 'glm-4.7'],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'volcengine-ark',
    labelKey: 'aiSettingsPreset_volcengineArk',
    draft: {
      name: '火山方舟 (Ark)',
      kind: 'openai-compatible-chat',
      baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
      model: 'doubao-seed-1-6-251015',
      models: [
        'doubao-seed-1-6-251015',
        'doubao-seed-1-6-250615',
        'doubao-seed-1-6-flash-250828',
        'doubao-seed-1-6-thinking-250715',
        'deepseek-v3-1-250821'
      ],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'siliconflow',
    labelKey: 'aiSettingsPreset_siliconflow',
    draft: {
      name: '硅基流动 (SiliconFlow)',
      kind: 'openai-compatible-chat',
      baseUrl: 'https://api.siliconflow.cn/v1',
      // SiliconFlow 文档示例的原始 model id；Pro 前缀为付费加速版。
      model: 'deepseek-ai/DeepSeek-V3',
      models: [
        'deepseek-ai/DeepSeek-V3',
        'Pro/deepseek-ai/DeepSeek-V3',
        'deepseek-ai/DeepSeek-R1',
        'Pro/deepseek-ai/DeepSeek-R1',
        'Qwen/Qwen2.5-72B-Instruct'
      ],
      allowNoAuth: false,
      allowInsecureHttp: false
    }
  },
  {
    id: 'ollama-local',
    labelKey: 'aiSettingsPreset_ollamaLocal',
    draft: {
      name: 'Ollama (本地)',
      kind: 'openai-compatible-chat',
      baseUrl: 'http://127.0.0.1:11434/v1',
      model: 'llama3.2',
      allowNoAuth: true,
      allowInsecureHttp: true
    }
  },
  {
    id: 'lm-studio-local',
    labelKey: 'aiSettingsPreset_lmStudioLocal',
    draft: {
      name: 'LM Studio (本地)',
      kind: 'openai-compatible-chat',
      baseUrl: 'http://127.0.0.1:1234/v1',
      model: 'loaded-model',
      allowNoAuth: true,
      allowInsecureHttp: true
    }
  }
]

// Settings can be rendered inline or through the modal portal. Share the
// action lock across both instances so a transition between those surfaces
// cannot submit the same provider operation twice.
let aiProviderActionInFlight = false

function aiProviderToDraft(provider: AiProviderSummary): AiProviderDraft {
  return {
    id: provider.id,
    name: provider.name,
    kind: provider.kind,
    baseUrl: provider.baseUrl,
    model: provider.model,
    models: provider.models,
    enabled: provider.enabled,
    isDefault: provider.isDefault,
    allowNoAuth: provider.allowNoAuth,
    allowInsecureHttp: provider.allowInsecureHttp
  }
}

function aiProviderRequestUrlPreview(draft: AiProviderDraft) {
  const endpoint =
    draft.kind === 'openai-compatible-chat'
      ? '/chat/completions'
      : draft.kind === 'openai-responses'
        ? '/responses'
        : '/messages'
  try {
    const url = new URL(draft.baseUrl.trim())
    // Keep this preview safe even while a user is correcting an invalid draft.
    // The Rust validator rejects credentials, queries and fragments on save.
    url.username = ''
    url.password = ''
    url.search = ''
    url.hash = ''
    return `${url.toString().replace(/\/+$/, '')}${endpoint}`
  } catch {
    return null
  }
}

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
  theme: 'default-dark' | 'default-light'
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  onSetTheme(value: 'default-dark' | 'default-light'): void
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
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab)
  const [syncSubTab, setSyncSubTab] = useState<'webdav' | 's3'>('webdav')
  const [agentSubTab, setAgentSubTab] = useState<'mcp' | 'cli'>('mcp')
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(true)
  const [updateChannel, setUpdateChannel] = useState<UiPreferences['updateChannel']>('stable')
  const [isSavingUpdatePreference, setIsSavingUpdatePreference] = useState(false)
  const [updatePreferenceError, setUpdatePreferenceError] = useState<string | null>(null)
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(false)
  const [isSavingTerminalZoomPreference, setIsSavingTerminalZoomPreference] = useState(false)
  const [terminalZoomPreferenceError, setTerminalZoomPreferenceError] = useState<string | null>(null)
  const [themeConfigOperation, setThemeConfigOperation] = useState<'import' | 'copy' | null>(null)
  const themeConfigOperationRef = useRef<typeof themeConfigOperation>(null)
  const [themeConfigMessage, setThemeConfigMessage] = useState<string | null>(null)
  const [customThemeName, setCustomThemeName] = useState('')
  const [editingCustomThemeId, setEditingCustomThemeId] = useState<string | null>(null)
  const [showDeleteThemeConfirm, setShowDeleteThemeConfirm] = useState(false)
  const [mcpAgentPreferences, setMcpAgentPreferences] = useState<McpAgentPreferences>(() => ({
    ...DEFAULT_MCP_AGENT_PREFERENCES
  }))
  const [mcpAgentSetup, setMcpAgentSetup] = useState<McpAgentSetup | null>(null)
  const [mcpAgentProfiles, setMcpAgentProfiles] = useState<ConnectionProfile[]>([])
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
  const [syncMessage, setSyncMessage] = useState<string | null>(null)
  const [s3Config, setS3Config] = useState<S3BackupConfig | null>(null)
  const [s3SecretAccessKey, setS3SecretAccessKey] = useState('')
  const [s3Message, setS3Message] = useState<string | null>(null)
  const [backupUploadMode, setBackupUploadMode] = useState<BackupUploadMode>('overwrite-cloud')
  const [backupDownloadMode, setBackupDownloadMode] = useState<BackupDownloadMode>('merge-local')
  const [aiProviders, setAiProviders] = useState<AiProviderSummary[]>([])
  const [aiDraft, setAiDraft] = useState<AiProviderDraft>(() => createAiProviderDraft())
  // Candidate model IDs carried by the currently applied preset. Cleared when
  // the user picks an already-configured provider (no preset bound). Stored
  // outside AiProviderDraft to keep the data-layer type free of UI-only state.
  const [aiModelChoices, setAiModelChoices] = useState<string[]>([])
  const [configuredModels, setConfiguredModels] = useState<string[]>([])
  const [selectedCandidateModel, setSelectedCandidateModel] = useState<string>('')
  const [isCustomInput, setIsCustomInput] = useState(false)
  const [customModelText, setCustomModelText] = useState('')
  const [aiApiKey, setAiApiKey] = useState('')
  const [clearAiApiKey, setClearAiApiKey] = useState(false)
  const [aiMessage, setAiMessage] = useState<string | null>(null)
  const [aiOperation, setAiOperation] = useState<'load' | 'save' | 'test' | 'delete' | null>(null)
  // React's disabled state is applied on the next render. Keep a synchronous
  // guard as well so rapid clicks cannot submit the same AI operation twice.
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
  const desktopApi = window.fileterm
  const updatePreviewState = import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined

  useEffect(() => {
    setActiveTab(initialTab)
  }, [initialTab])

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
    void desktopApi
      .getUiPreferences()
      .then((preferences) => {
        if (!canceled) {
          setAutoCheckUpdates(preferences.autoCheckUpdates)
          setUpdateChannel(preferences.updateChannel)
          setTerminalZoomLocked(preferences.terminalZoomLocked)
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
      })
      .catch(() => {
        if (!canceled) {
          setUpdatePreferenceError(t.updatePreferenceLoadFailed)
        }
      })

    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) {
        setAutoCheckUpdates(preferences.autoCheckUpdates)
        setUpdateChannel(preferences.updateChannel)
        setTerminalZoomLocked(preferences.terminalZoomLocked)
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
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi])

  useEffect(() => {
    if (activeTab !== 'sync' || !desktopApi) return
    if (syncOperationRef.current) return
    syncOperationRef.current = 'load'
    setSyncOperation('load')
    void desktopApi
      .getWebDavSyncConfig()
      .then(async (webDavConfig) => {
        setSyncConfig(webDavConfig)
        setS3Config(await desktopApi.getS3BackupConfig())
      })
      .catch((error: unknown) => setSyncMessage(error instanceof Error ? error.message : String(error)))
      .finally(() => {
        if (syncOperationRef.current === 'load') {
          syncOperationRef.current = null
          setSyncOperation(null)
        }
      })
  }, [activeTab, desktopApi])

  useEffect(() => {
    if (activeTab !== 'agent') return
    if (!desktopApi) {
      setMcpAgentMessage(t.agentMcpDesktopOnly)
      return
    }
    let canceled = false
    setMcpAgentOperation('load')
    setMcpAgentMessage(null)
    void Promise.all([desktopApi.getMcpAgentSetup(), desktopApi.getConnectionLibrary(), desktopApi.getUiPreferences()])
      .then(([setup, library, preferences]) => {
        if (canceled) return
        setMcpAgentSetup(setup)
        setMcpAgentProfiles(library.profiles)
        setMcpAgentPreferences({ ...DEFAULT_MCP_AGENT_PREFERENCES, ...preferences.mcpAgent })
      })
      .catch((error: unknown) => {
        if (!canceled) {
          setMcpAgentMessage(error instanceof Error ? error.message : String(error))
        }
      })
      .finally(() => {
        if (!canceled) setMcpAgentOperation(null)
      })
    return () => {
      canceled = true
    }
  }, [activeTab, desktopApi])

  useEffect(() => {
    if (activeTab !== 'ai') return
    if (!desktopApi) {
      setAiMessage(t.aiSettingsDesktopOnly)
      return
    }

    let canceled = false
    setAiOperation('load')
    setAiMessage(null)
    void desktopApi
      .listAiProviders()
      .then((providers) => {
        if (canceled) return
        setAiProviders(providers)
        const current = aiDraft.id ? providers.find((provider) => provider.id === aiDraft.id) : undefined
        const nextProvider = current ?? providers.find((provider) => provider.isDefault) ?? providers[0]
        // Use selectAiProvider to restore configuredModels and aiModelChoices properly
        selectAiProvider(nextProvider)
        setAiApiKey('')
      })
      .catch((error: unknown) => {
        if (!canceled) {
          setAiMessage(error instanceof Error ? error.message : String(error))
        }
      })
      .finally(() => {
        if (!canceled) {
          setAiOperation(null)
        }
      })

    return () => {
      canceled = true
    }
    // selectAiProvider is a stable inline function and intentionally not listed
    // in deps — this effect runs once per tab activation, matching original behaviour.
  }, [activeTab, desktopApi])

  const runSyncOperation = async (
    operation: Exclude<typeof syncOperation, 'load' | null>,
    action: () => Promise<void>
  ) => {
    if (syncOperationRef.current) return
    syncOperationRef.current = operation
    setSyncOperation(operation)
    if (operation.startsWith('s3-')) {
      setS3Message(null)
    } else {
      setSyncMessage(null)
    }
    try {
      await action()
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (operation.startsWith('s3-')) {
        setS3Message(message)
      } else {
        setSyncMessage(message)
      }
    } finally {
      if (syncOperationRef.current === operation) {
        syncOperationRef.current = null
        setSyncOperation(null)
      }
    }
  }

  const patchAiDraft = (patch: Partial<AiProviderDraft>) => {
    setAiDraft((current) => ({ ...current, ...patch }))
  }

  const selectAiProvider = (provider: AiProviderSummary | undefined) => {
    const draft = provider ? aiProviderToDraft(provider) : createAiProviderDraft(aiProviders.length === 0)
    if (!provider) {
      draft.model = ''
    }
    setAiDraft(draft)
    const presetMatch = AI_PROVIDER_PRESETS.find(
      (p) => p.draft.baseUrl === draft.baseUrl || p.draft.name.toLowerCase() === draft.name.toLowerCase()
    )
    const defaultModels = presetMatch?.draft.models ?? DEFAULT_MODELS_BY_KIND[draft.kind] ?? []
    setAiModelChoices([...new Set([draft.model, ...defaultModels].filter(Boolean))])
    const providerModels =
      provider?.models && provider.models.length > 0 ? provider.models : provider?.model ? [provider.model] : []
    setConfiguredModels(providerModels)
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
    setAiApiKey('')
    setClearAiApiKey(false)
    setAiMessage(null)
  }

  // Apply a curated preset without wiping the draft's identity (`id`) or the
  // user's `enabled` / `isDefault` choices. The API key stays untouched.
  const applyAiPreset = (preset: AiProviderPreset) => {
    setAiDraft((current) => ({
      ...current,
      name: preset.draft.name,
      kind: preset.draft.kind,
      baseUrl: preset.draft.baseUrl,
      model: '',
      allowNoAuth: preset.draft.allowNoAuth,
      allowInsecureHttp: preset.draft.allowInsecureHttp
    }))
    const presetModels = preset.draft.models ?? (preset.draft.model ? [preset.draft.model] : [])
    setAiModelChoices(presetModels)
    setConfiguredModels([])
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
    setAiMessage(null)
  }

  const addSelectedModelToProvider = () => {
    let modelToAdd = selectedCandidateModel.trim()
    if (isCustomInput) {
      modelToAdd = customModelText.trim()
    }
    if (!modelToAdd) return

    setConfiguredModels((prev) => [...new Set([...prev, modelToAdd])])
    setAiModelChoices((prev) => [...new Set([modelToAdd, ...prev])])
    patchAiDraft({ model: modelToAdd })
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
  }

  const removeConfiguredModel = (modelName: string) => {
    setConfiguredModels((prev) => {
      const next = prev.filter((m) => m !== modelName)
      if (aiDraft.model === modelName) {
        patchAiDraft({ model: next[0] ?? '' })
      }
      return next
    })
  }

  const candidateModelOptions = useMemo(
    () => [...new Set([...aiModelChoices, ...configuredModels].filter(Boolean))],
    [aiModelChoices, configuredModels]
  )

  const aiProviderInput = () => {
    const secrets = clearAiApiKey ? { apiKey: null } : aiApiKey.trim() ? { apiKey: aiApiKey } : undefined
    const activeModel = aiDraft.model || configuredModels[0] || ''
    return {
      provider: {
        ...aiDraft,
        name: aiDraft.name.trim(),
        model: activeModel,
        models: configuredModels
      },
      ...(secrets ? { secrets } : {})
    }
  }

  const saveAiProvider = async () => {
    if (!desktopApi || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return
    const trimmedName = aiDraft.name.trim()
    if (!trimmedName) {
      setAiMessage('Provider 名称不能为空')
      return
    }
    const duplicate = aiProviders.find(
      (p) => p.name.trim().toLowerCase() === trimmedName.toLowerCase() && p.id !== aiDraft.id
    )
    if (duplicate) {
      setAiMessage(`Provider 名称 "${trimmedName}" 已存在，请使用其他唯一名称`)
      return
    }
    if (configuredModels.length === 0) {
      setAiMessage('请至少添加一个模型到 Provider')
      return
    }

    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    // Keep the current footer message while the request is in flight. Clearing
    // it would briefly render the idle test hint for fast save requests.
    setAiOperation('save')
    try {
      const saved = await desktopApi.saveAiProvider(aiProviderInput())
      const providers = await desktopApi.listAiProviders()
      setAiProviders(providers)
      const selected = providers.find((provider) => provider.id === saved.id) ?? saved
      setAiDraft(aiProviderToDraft(selected))
      const savedModels =
        selected.models && selected.models.length > 0 ? selected.models : selected.model ? [selected.model] : []
      setConfiguredModels(savedModels)
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage(t.aiSettingsSaveSucceeded)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiApiKey('')
      setClearAiApiKey(false)
      setAiOperation(null)
    }
  }

  const testAiProvider = async () => {
    if (!desktopApi || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return
    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    setAiOperation('test')
    try {
      const result = await desktopApi.testAiProvider(aiProviderInput())
      setAiMessage(result.message)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiOperation(null)
    }
  }

  const deleteAiProvider = async () => {
    if (!desktopApi || !aiDraft.id || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return

    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    setAiOperation('delete')
    try {
      const providers = await desktopApi.deleteAiProvider(aiDraft.id)
      setAiProviders(providers)
      const fallback = providers.find((provider) => provider.isDefault) ?? providers[0]
      selectAiProvider(fallback)
      setAiApiKey('')
      setClearAiApiKey(false)
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage(t.aiSettingsDeleteSucceeded)
      setShowDeleteAiProviderConfirm(false)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiOperation(null)
    }
  }

  const selectedAiProvider = aiDraft.id ? aiProviders.find((provider) => provider.id === aiDraft.id) : undefined
  const aiRequestUrlPreview = aiProviderRequestUrlPreview(aiDraft)

  const platformLabel = (() => {
    const platform = desktopApi?.platform ?? 'unknown'
    const arch = desktopApi?.arch ?? 'unknown'
    if (platform === 'darwin') {
      if (arch === 'arm64') return 'macOS (Apple Silicon)'
      if (arch === 'x64' || arch === 'x86_64') return 'macOS (Intel)'
      return `macOS (${arch})`
    }
    if (platform === 'win32') {
      return arch === 'arm64' ? 'Windows (ARM)' : `Windows (${arch})`
    }
    if (platform === 'linux') {
      return `Linux (${arch})`
    }
    return `${platform} / ${arch}`
  })()

  const managerToolsHint = inline ? t.settingsManagersInlineHint : t.settingsManagersWindowHint
  const managerToolsActionLabel = inline ? t.switchToManagerPage : t.openInSeparateWindow

  const setUpdateCheckPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingUpdatePreference || nextValue === autoCheckUpdates) {
      return
    }

    const previousValue = autoCheckUpdates
    setAutoCheckUpdates(nextValue)
    setUpdatePreferenceError(null)
    setIsSavingUpdatePreference(true)
    void desktopApi
      .setUiPreferences({ autoCheckUpdates: nextValue })
      .then((preferences) => setAutoCheckUpdates(preferences.autoCheckUpdates))
      .catch(() => {
        setAutoCheckUpdates(previousValue)
        setUpdatePreferenceError(t.updatePreferenceSaveFailed)
      })
      .finally(() => setIsSavingUpdatePreference(false))
  }

  const setUpdateChannelPreference = (nextValue: UiPreferences['updateChannel']) => {
    if (!desktopApi || isSavingUpdatePreference || nextValue === updateChannel) {
      return
    }

    const previousValue = updateChannel
    setUpdateChannel(nextValue)
    setUpdatePreferenceError(null)
    setIsSavingUpdatePreference(true)
    void desktopApi
      .setUiPreferences({ updateChannel: nextValue })
      .then((preferences) => {
        setUpdateChannel(preferences.updateChannel)
        void desktopApi.checkForUpdates().catch(() => undefined)
      })
      .catch(() => {
        setUpdateChannel(previousValue)
        setUpdatePreferenceError(t.updatePreferenceSaveFailed)
      })
      .finally(() => setIsSavingUpdatePreference(false))
  }

  const setConnectionDefault = <K extends keyof SshConnectionDefaults>(key: K, value: SshConnectionDefaults[K]) => {
    if (!desktopApi || isSavingConnectionDefaults || connectionDefaults[key] === value) {
      return
    }

    const previousDefaults = connectionDefaults
    const nextDefaults = { ...connectionDefaults, [key]: value }
    setConnectionDefaults(nextDefaults)
    setConnectionDefaultsError(null)
    setIsSavingConnectionDefaults(true)
    void desktopApi
      .setUiPreferences({ connectionDefaults: { [key]: value } })
      .then((preferences) =>
        setConnectionDefaults({ ...DEFAULT_SSH_CONNECTION_DEFAULTS, ...preferences.connectionDefaults })
      )
      .catch(() => {
        setConnectionDefaults(previousDefaults)
        setConnectionDefaultsError(t.connectionDefaultsSaveFailed)
      })
      .finally(() => setIsSavingConnectionDefaults(false))
  }

  const setTerminalZoomLockPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingTerminalZoomPreference || nextValue === terminalZoomLocked) {
      return
    }

    const previousValue = terminalZoomLocked
    setTerminalZoomLocked(nextValue)
    setTerminalZoomPreferenceError(null)
    setIsSavingTerminalZoomPreference(true)
    void desktopApi
      .setUiPreferences({ terminalZoomLocked: nextValue })
      .then((preferences) => setTerminalZoomLocked(preferences.terminalZoomLocked))
      .catch(() => {
        setTerminalZoomLocked(previousValue)
        setTerminalZoomPreferenceError(t.terminalZoomPreferenceSaveFailed)
      })
      .finally(() => setIsSavingTerminalZoomPreference(false))
  }

  const themeVariant = theme === 'default-light' ? 'light' : 'dark'

  const setThemeConfigValue = (nextValue: ThemeConfig) => {
    onSetThemeConfig(
      normalizeThemeConfig(
        {
          ...nextValue,
          codeThemeId: 'custom',
          baseThemeId: themeBaseIdForConfig(themeConfig)
        },
        themeVariant
      )
    )
    setThemeConfigMessage(null)
  }

  const updateThemeBody = (patch: Partial<ThemeConfig['theme']>) => {
    setThemeConfigValue({
      ...themeConfig,
      theme: {
        ...themeConfig.theme,
        ...patch
      }
    })
  }

  const updateThemeSemanticColors = (patch: Partial<ThemeConfig['theme']['semanticColors']>) => {
    updateThemeBody({
      semanticColors: {
        ...themeConfig.theme.semanticColors,
        ...patch
      }
    })
  }

  const updateThemeFonts = (patch: Partial<ThemeConfig['theme']['fonts']>) => {
    updateThemeBody({
      fonts: {
        ...themeConfig.theme.fonts,
        ...patch
      }
    })
  }

  const updateTerminalTheme = (patch: Partial<ThemeConfig['theme']['terminal']>) => {
    updateThemeBody({
      terminal: {
        ...themeConfig.theme.terminal,
        ...patch
      }
    })
  }

  const updateTerminalAnsiColor = (name: TerminalAnsiColorName, value: string) => {
    updateTerminalTheme({
      ansi: {
        ...themeConfig.theme.terminal.ansi,
        [name]: value
      }
    })
  }

  const updateTerminalSearchColors = (patch: Partial<ThemeConfig['theme']['terminal']['search']>) => {
    updateTerminalTheme({
      search: {
        ...themeConfig.theme.terminal.search,
        ...patch
      }
    })
  }

  const applyThemePreset = (presetId: string, variant: ThemePresetVariant = themeVariant) => {
    if (presetId.startsWith('saved:')) {
      const savedId = presetId.slice('saved:'.length)
      const savedTheme = customThemes.find((candidate) => candidate.id === savedId)
      if (!savedTheme) return
      const nextThemeConfig = normalizeThemeConfig(savedTheme.config, savedTheme.config.variant)
      onSetThemeConfig(nextThemeConfig)
      onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
      setEditingCustomThemeId(savedTheme.id)
      setCustomThemeName(savedTheme.name)
      setThemeConfigMessage(t.themePresetApplied)
      return
    }

    const preset = THEME_PRESETS.find((candidate) => candidate.id === presetId)
    if (!preset) return
    const nextThemeConfig = normalizeThemeConfig(preset.config[variant], variant)
    onSetThemeConfig(nextThemeConfig)
    onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
    setEditingCustomThemeId(null)
    setCustomThemeName('')
    setThemeConfigMessage(t.themePresetApplied)
  }

  const saveCustomTheme = () => {
    const name = customThemeName.trim()
    if (!name) {
      setThemeConfigMessage(t.themeNameRequired)
      return
    }

    const nextThemeConfig = normalizeThemeConfig(
      {
        ...themeConfig,
        codeThemeId: 'custom',
        baseThemeId: themeBaseIdForConfig(themeConfig)
      },
      themeVariant
    )
    const existingTheme = editingCustomThemeId
      ? customThemes.find((candidate) => candidate.id === editingCustomThemeId)
      : undefined
    const id = existingTheme?.id ?? createCustomThemeId()
    const nextCustomThemes = [
      ...customThemes.filter((candidate) => candidate.id !== id),
      { id, name, config: nextThemeConfig }
    ]

    onSetCustomThemes(nextCustomThemes)
    onSetThemeConfig(nextThemeConfig)
    onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
    setEditingCustomThemeId(id)
    setCustomThemeName(name)
    setThemeConfigMessage(existingTheme ? t.themeUpdated : t.themeSaved)
  }

  const deleteCustomTheme = () => {
    if (!selectedSavedTheme) return
    const idToDelete = selectedSavedTheme.id
    const nextCustomThemes = customThemes.filter((candidate) => candidate.id !== idToDelete)
    onSetCustomThemes(nextCustomThemes)
    setEditingCustomThemeId(null)
    setCustomThemeName('')
    applyThemePreset('fileterm')
    setThemeConfigMessage(t.themeDeleted)
    setShowDeleteThemeConfirm(false)
  }

  const switchThemeVariant = (nextVariant: ThemePresetVariant) => {
    if (themeConfig.variant === nextVariant) return

    const matchingPreset = findMatchingThemePreset(themeConfig)
    if (matchingPreset) {
      applyThemePreset(matchingPreset.id, nextVariant)
      return
    }

    const isCodexTheme = themeConfig.codeThemeId === 'codex' || themeConfig.codeThemeId.startsWith('codex-')
    const isFileTermTheme =
      themeConfig.codeThemeId === 'fileterm' ||
      themeConfig.codeThemeId === 'fileterm-dark' ||
      themeConfig.codeThemeId === 'fileterm-light'
    if (isCodexTheme) {
      applyThemePreset('codex', nextVariant)
      return
    }
    if (isFileTermTheme) {
      applyThemePreset('fileterm', nextVariant)
      return
    }

    onSetTheme(nextVariant === 'light' ? 'default-light' : 'default-dark')
    onSetThemeConfig(normalizeThemeConfig({ ...themeConfig, variant: nextVariant }, nextVariant))
  }

  const parseImportedTheme = (text: string): unknown => {
    const trimmed = text.trim()
    const payload = THEME_CONFIG_IMPORT_PREFIXES.reduce(
      (value, prefix) => (value.startsWith(prefix) ? value.slice(prefix.length) : value),
      trimmed
    )
    const jsonStart = payload.indexOf('{')
    const jsonEnd = payload.lastIndexOf('}')
    if (jsonStart < 0 || jsonEnd <= jsonStart) {
      throw new Error('Theme JSON was not found')
    }
    return JSON.parse(payload.slice(jsonStart, jsonEnd + 1)) as unknown
  }

  const readThemeClipboard = async () => {
    let lastError: unknown = null
    if (desktopApi?.readClipboardText) {
      try {
        return await desktopApi.readClipboardText()
      } catch (error) {
        lastError = error
      }
    }
    if (navigator.clipboard?.readText) {
      try {
        return await navigator.clipboard.readText()
      } catch (error) {
        lastError = error
      }
    }
    throw clipboardUnavailableError(lastError)
  }

  const writeThemeClipboard = async (text: string) => {
    let lastError: unknown = null
    if (desktopApi?.writeClipboardText) {
      try {
        await desktopApi.writeClipboardText(text)
        return
      } catch (error) {
        lastError = error
      }
    }
    if (navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(text)
        return
      } catch (error) {
        lastError = error
      }
    }

    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.setAttribute('readonly', '')
    textarea.setAttribute('aria-hidden', 'true')
    textarea.style.position = 'fixed'
    textarea.style.top = '0'
    textarea.style.left = '-9999px'
    textarea.style.opacity = '0'
    document.body.appendChild(textarea)
    textarea.select()
    try {
      if (document.execCommand('copy')) return
    } catch (error) {
      lastError = error
    } finally {
      document.body.removeChild(textarea)
    }
    throw clipboardUnavailableError(lastError)
  }

  const beginThemeConfigOperation = (operation: NonNullable<typeof themeConfigOperation>) => {
    if (themeConfigOperationRef.current) return false
    themeConfigOperationRef.current = operation
    setThemeConfigOperation(operation)
    setThemeConfigMessage(null)
    return true
  }

  const endThemeConfigOperation = () => {
    themeConfigOperationRef.current = null
    setThemeConfigOperation(null)
  }

  const importThemeConfig = async () => {
    if (!beginThemeConfigOperation('import')) return
    try {
      const clipboardText = await readThemeClipboard()
      const importedTheme = normalizeThemeConfig(parseImportedTheme(clipboardText), themeVariant)
      onSetThemeConfig(importedTheme)
      onSetTheme(importedTheme.variant === 'light' ? 'default-light' : 'default-dark')
      setEditingCustomThemeId(null)
      setCustomThemeName('')
      setThemeConfigMessage(t.themeImported)
    } catch {
      setThemeConfigMessage(t.themeImportFailed)
    } finally {
      endThemeConfigOperation()
    }
  }

  const copyThemeConfig = async () => {
    if (!beginThemeConfigOperation('copy')) return
    try {
      const normalizedTheme = normalizeThemeConfig({ ...themeConfig, variant: themeVariant }, themeVariant)
      const serializedTheme = `${THEME_CONFIG_EXPORT_PREFIX}${JSON.stringify({
        codeThemeId: normalizedTheme.codeThemeId,
        baseThemeId: normalizedTheme.baseThemeId,
        theme: normalizedTheme.theme,
        variant: normalizedTheme.variant
      })}`
      await writeThemeClipboard(serializedTheme)
      setThemeConfigMessage(t.themeCopied)
    } catch {
      setThemeConfigMessage(t.themeCopyFailed)
    } finally {
      endThemeConfigOperation()
    }
  }

  const saveMcpAgentPreferences = (patch: Partial<McpAgentPreferences>) => {
    if (!desktopApi || mcpAgentOperation === 'save') {
      return
    }

    const previousPreferences = mcpAgentPreferences
    const nextPreferences = { ...mcpAgentPreferences, ...patch }
    if (
      nextPreferences.connectionScope === previousPreferences.connectionScope &&
      nextPreferences.operationPolicy === previousPreferences.operationPolicy &&
      nextPreferences.defaultProfileId === previousPreferences.defaultProfileId
    ) {
      return
    }
    setMcpAgentPreferences(nextPreferences)
    setMcpAgentMessage(null)
    setMcpAgentOperation('save')
    void desktopApi
      .setUiPreferences({ mcpAgent: nextPreferences })
      .then((preferences) => {
        setMcpAgentPreferences({ ...DEFAULT_MCP_AGENT_PREFERENCES, ...preferences.mcpAgent })
        setMcpAgentMessage(t.agentMcpSaved)
      })
      .catch((error: unknown) => {
        setMcpAgentPreferences(previousPreferences)
        setMcpAgentMessage(error instanceof Error ? error.message : String(error))
      })
      .finally(() => setMcpAgentOperation(null))
  }

  const copyMcpAgentCommand = (command: string, successMessage: string) => {
    if (!desktopApi || !command) return
    setMcpAgentMessage(null)
    void desktopApi
      .writeClipboardText(command)
      .then(() => setMcpAgentMessage(successMessage))
      .catch((error: unknown) => setMcpAgentMessage(error instanceof Error ? error.message : String(error)))
  }

  const copyMcpAgentRegistrationCommand = (command: string) => {
    copyMcpAgentCommand(command, t.agentMcpCommandCopied)
  }

  const launchMcpAgentInLocalTerminal = (client: McpAgentClientStatus) => {
    if (!client.available || !onLaunchLocalAgent) return
    setMcpAgentMessage(null)
    onLaunchLocalAgent(client)
  }

  const applyOverviewPreferences = (preferences: UiPreferences) => {
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

  const setOverviewShowStatsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowStats) {
      return
    }

    const previousValue = overviewShowStats
    setOverviewShowStats(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowStats: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowStats(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowRecentPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowRecent) {
      return
    }

    const previousValue = overviewShowRecent
    setOverviewShowRecent(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowRecent: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowRecent(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowAllConnectionsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowAllConnections) {
      return
    }

    const previousValue = overviewShowAllConnections
    setOverviewShowAllConnections(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowAllConnections: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowAllConnections(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const setOverviewShowQuickActionsPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingOverviewPreference || nextValue === overviewShowQuickActions) {
      return
    }

    const previousValue = overviewShowQuickActions
    setOverviewShowQuickActions(nextValue)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewShowQuickActions: nextValue })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewShowQuickActions(previousValue)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const clearOverviewDragState = () => {
    overviewDragStateRef.current = { source: null, target: null, position: null }
    setDraggingOverviewSection(null)
    setDragOverOverviewSection(null)
    setOverviewDragPosition(null)
    window.setTimeout(() => {
      suppressOverviewCardClickRef.current = false
    }, 0)
  }

  const setOverviewDropTarget = (target: OverviewSectionId, position: ManagerDropPosition) => {
    if (overviewDragStateRef.current.target === target && overviewDragStateRef.current.position === position) {
      return
    }

    overviewDragStateRef.current.target = target
    overviewDragStateRef.current.position = position
    setDragOverOverviewSection(target)
    setOverviewDragPosition(position)
  }

  const positionForOverviewTarget = (target: PointerSortTarget | HTMLElement, clientY: number) => {
    if ('kind' in target && target.kind === 'overview-section-top') {
      return 'top' as const
    }

    const element = 'element' in target ? target.element : target
    return resolveManagerDropPosition(element, clientY, false)
  }

  const persistOverviewSectionOrder = (nextOrder: OverviewSectionId[], previousOrder: OverviewSectionId[]) => {
    if (!desktopApi || isSavingOverviewPreference) return

    setOverviewSectionOrder(nextOrder)
    setOverviewPreferenceError(null)
    setIsSavingOverviewPreference(true)
    void desktopApi
      .setUiPreferences({ overviewSectionOrder: nextOrder })
      .then(applyOverviewPreferences)
      .catch(() => {
        setOverviewSectionOrder(previousOrder)
        setOverviewPreferenceError(t.overviewPreferenceSaveFailed)
      })
      .finally(() => setIsSavingOverviewPreference(false))
  }

  const applyOverviewSectionDrop = (
    source: OverviewSectionId,
    target: OverviewSectionId,
    position: ManagerDropPosition
  ) => {
    if (source === target || position === 'inside' || isSavingOverviewPreference) return

    const previousOrder = overviewSectionOrder
    const nextOrder = overviewSectionOrder.filter((sectionId) => sectionId !== source)
    const targetIndex = nextOrder.indexOf(target)
    if (targetIndex === -1) return

    nextOrder.splice(position === 'bottom' ? targetIndex + 1 : targetIndex, 0, source)
    if (nextOrder.every((sectionId, index) => sectionId === previousOrder[index])) return
    persistOverviewSectionOrder(nextOrder, previousOrder)
  }

  const handleOverviewPointerDown = usePointerSortFallback<OverviewSectionId>({
    onStart: (sectionId) => {
      if (isSavingOverviewPreference) return
      suppressOverviewCardClickRef.current = true
      overviewDragStateRef.current = { source: sectionId, target: null, position: null }
      setDraggingOverviewSection(sectionId)
    },
    onTarget: (source, target, clientY) => {
      if (source === target.id || (target.kind !== 'overview-section' && target.kind !== 'overview-section-top')) {
        return
      }
      setOverviewDropTarget(target.id as OverviewSectionId, positionForOverviewTarget(target, clientY))
    },
    onDrop: (source, target, clientY) => {
      if (
        target &&
        (target.kind === 'overview-section' || target.kind === 'overview-section-top') &&
        source !== target.id
      ) {
        applyOverviewSectionDrop(source, target.id as OverviewSectionId, positionForOverviewTarget(target, clientY))
      }
      clearOverviewDragState()
    },
    onCancel: clearOverviewDragState
  })

  const overviewSectionMeta: Record<OverviewSectionId, { title: string; hint: string }> = {
    stats: { title: t.overviewShowStats, hint: t.overviewShowStatsHint },
    recent: { title: t.overviewShowRecent, hint: t.overviewShowRecentHint },
    allConnections: { title: t.overviewShowAllConnections, hint: t.overviewShowAllConnectionsHint },
    quickActions: { title: t.overviewShowQuickActions, hint: t.overviewShowQuickActionsHint }
  }

  const selectedThemePreset = findMatchingThemePreset(themeConfig)
  const matchingSavedTheme = customThemes.find((candidate) => sameThemeConfig(candidate.config, themeConfig))
  const editingSavedTheme = editingCustomThemeId
    ? customThemes.find((candidate) => candidate.id === editingCustomThemeId)
    : undefined
  const selectedSavedTheme = editingSavedTheme ?? matchingSavedTheme
  const themePresetValue = selectedSavedTheme ? `saved:${selectedSavedTheme.id}` : (selectedThemePreset?.id ?? 'custom')
  const themePresetLabel = selectedSavedTheme
    ? selectedSavedTheme.name
    : selectedThemePreset
      ? t[selectedThemePreset.labelKey]
      : t.themePresetCustom
  const themePresetCode = selectedSavedTheme || !selectedThemePreset ? 'custom' : selectedThemePreset.id

  useEffect(() => {
    if (!editingCustomThemeId && matchingSavedTheme) {
      setEditingCustomThemeId(matchingSavedTheme.id)
      setCustomThemeName(matchingSavedTheme.name)
    }
  }, [editingCustomThemeId, matchingSavedTheme])

  const content = (
    <div
      className={`modal-card manager-modal connection-manager-modal settings-modal ${standalone ? 'standalone' : ''} ${inline ? 'manager-inline' : ''}`}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">settings</span>
          <span>{t.settings}</span>
        </span>
        {!inline && (
          <div className="connection-manager-header-actions">
            <CloseButton disabled={syncOperation !== null} onClick={onClose} />
          </div>
        )}
      </div>
      <div className="connection-manager-layout">
        <aside className="connection-manager-sidebar" aria-label={t.settings}>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'interface' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('interface')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">palette</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.interfaceSettings}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'ai' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('ai')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">auto_awesome</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.aiSettings}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'agent' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('agent')}
          >
            <span className="connection-manager-sidebar-icon">
              <AppIcon name="terminal-file" size={17} />
            </span>
            <span className="connection-manager-sidebar-label">{t.agentMcpSettings}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'connections' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('connections')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">settings_ethernet</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.connectionDefaults}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'sync' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('sync')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">cloud_sync</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.configSync}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'updates' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('updates')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">system_update</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.appUpdates}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'tools' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('tools')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">apps</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.managerToolsShortcut}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'system' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('system')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">info</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.systemLogsInfo}</span>
          </button>
          <button
            className={`connection-manager-sidebar-item ${activeTab === 'language' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveTab('language')}
          >
            <span className="connection-manager-sidebar-icon">
              <span className="material-symbols-outlined">translate</span>
            </span>
            <span className="connection-manager-sidebar-label">{t.languageSidebarLabel}</span>
          </button>
        </aside>

        <main className="connection-manager-main">
          {activeTab === 'ai' ? (
            <div className="settings-panel settings-ai-panel">
              <section className="settings-section">
                <h3>{t.aiSettingsProvider}</h3>
                <p className="settings-tools-hint">{t.aiSettingsProviderDescription}</p>

                <div className="ai-settings-provider-card">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    auto_awesome
                  </span>
                  <div>
                    <strong>
                      {selectedAiProvider?.usable
                        ? t.aiSettingsProviderReady
                        : selectedAiProvider
                          ? t.aiSettingsProviderNeedsAttention
                          : t.aiSettingsNotConfigured}
                    </strong>
                    <p>
                      {aiRequestUrlPreview
                        ? `${t.aiSettingsRequestUrlPreview} · ${aiRequestUrlPreview}`
                        : selectedAiProvider
                          ? selectedAiProvider.baseUrl
                          : t.aiSettingsPreviewHint}
                    </p>
                  </div>
                  <span className="ai-settings-preview-tag">
                    {selectedAiProvider?.hasApiKey ? t.aiSettingsApiKeySaved : t.aiCopilotPreview}
                  </span>
                </div>

                <div className="ai-settings-provider-picker">
                  <label>
                    <span>{t.aiSettingsConfiguredProviders}</span>
                    <DropdownSelect
                      disabled={!desktopApi || aiOperation !== null}
                      value={aiDraft.id ?? '__new__'}
                      options={[
                        ...aiProviders.map((provider) => ({ value: provider.id, label: provider.name })),
                        { value: '__new__', label: t.aiSettingsAddProvider }
                      ]}
                      onChange={(providerId) => {
                        selectAiProvider(
                          providerId === '__new__'
                            ? undefined
                            : aiProviders.find((provider) => provider.id === providerId)
                        )
                      }}
                    />
                  </label>
                  <button
                    className="ai-settings-secondary-button"
                    disabled={!desktopApi || aiOperation !== null}
                    type="button"
                    onClick={() => selectAiProvider(undefined)}
                  >
                    <AppIcon name="plus" size={14} />
                    {t.aiSettingsAddProvider}
                  </button>
                </div>

                {!aiDraft.id && (
                  <div className="ai-settings-preset-picker">
                    <span className="ai-settings-preset-label">{t.aiSettingsPresetLabel}</span>
                    <DropdownSelect
                      className="ai-settings-preset-select"
                      disabled={!desktopApi || aiOperation !== null}
                      value="__none__"
                      placeholder={t.aiSettingsPresetPlaceholder}
                      options={[
                        { value: '__none__', label: t.aiSettingsPresetPlaceholder },
                        ...AI_PROVIDER_PRESETS.map((preset) => ({
                          value: preset.id,
                          label: String(t[preset.labelKey])
                        }))
                      ]}
                      onChange={(value) => {
                        const preset = AI_PROVIDER_PRESETS.find((item) => item.id === value)
                        if (preset) {
                          applyAiPreset(preset)
                        }
                      }}
                    />
                    <p className="ai-settings-preset-hint">{t.aiSettingsPresetHint}</p>
                  </div>
                )}

                <fieldset className="ai-settings-provider-fields" disabled={!desktopApi || aiOperation !== null}>
                  <div className="ai-settings-form">
                    <label>
                      <span>{t.aiSettingsProviderName}</span>
                      <input
                        placeholder={t.aiSettingsProviderNamePlaceholder}
                        value={aiDraft.name}
                        onChange={(event) => patchAiDraft({ name: event.target.value })}
                      />
                    </label>
                    <label>
                      <span>{t.aiSettingsProviderType}</span>
                      <DropdownSelect
                        disabled={!desktopApi || aiOperation !== null}
                        value={aiDraft.kind}
                        options={[
                          { value: 'openai-compatible-chat', label: 'OpenAI-compatible Chat (OpenAI 兼容对话协议)' },
                          { value: 'openai-responses', label: 'OpenAI Responses (OpenAI 官方结构化响应协议)' },
                          { value: 'anthropic-messages', label: 'Anthropic Messages (Claude 官方消息协议)' }
                        ]}
                        onChange={(value) => patchAiDraft({ kind: value as AiProviderKind })}
                      />
                    </label>
                    <label className="ai-settings-model-field">
                      <div className="ai-settings-model-header">
                        <span>{t.aiSettingsModel}</span>
                        <small className="ai-settings-model-picker-hint">{t.aiSettingsModelAddHint}</small>
                      </div>
                      <div className="ai-settings-model-container">
                        <div className="ai-settings-model-left">
                          {isCustomInput ? (
                            <div className="ai-settings-model-add-row">
                              <input
                                autoFocus
                                className="ai-settings-model-add-input"
                                placeholder="输入自定义模型名称 (例: deepseek-r1)"
                                value={customModelText}
                                onChange={(e) => setCustomModelText(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') {
                                    e.preventDefault()
                                    addSelectedModelToProvider()
                                  } else if (e.key === 'Escape') {
                                    setIsCustomInput(false)
                                  }
                                }}
                              />
                              <button
                                className="ai-settings-secondary-button ai-settings-add-model-btn"
                                disabled={!customModelText.trim()}
                                type="button"
                                onClick={addSelectedModelToProvider}
                                title="添加此自定义模型到 Provider"
                              >
                                <AppIcon name="plus" size={14} />
                              </button>
                              <button
                                className="ai-settings-secondary-button ai-settings-model-cancel-btn"
                                type="button"
                                onClick={() => setIsCustomInput(false)}
                                title="取消"
                              >
                                取消
                              </button>
                            </div>
                          ) : (
                            <div className="ai-settings-model-picker-row">
                              <DropdownSelect
                                className="ai-settings-model-select"
                                disabled={!desktopApi || aiOperation !== null}
                                value={selectedCandidateModel || '__none__'}
                                options={[
                                  { value: '__none__', label: '选择模型以添加...' },
                                  ...candidateModelOptions.map((model: string) => ({
                                    value: model,
                                    label: model
                                  })),
                                  { value: '__custom__', label: '+ 自定义模型...' }
                                ]}
                                onChange={(value) => {
                                  if (value === '__custom__') {
                                    setIsCustomInput(true)
                                    setSelectedCandidateModel('')
                                    setCustomModelText('')
                                  } else if (value === '__none__') {
                                    setSelectedCandidateModel('')
                                  } else {
                                    setSelectedCandidateModel(value)
                                    setIsCustomInput(false)
                                  }
                                }}
                              />
                              <button
                                className="ai-settings-secondary-button ai-settings-add-model-btn"
                                disabled={!desktopApi || aiOperation !== null || !selectedCandidateModel}
                                type="button"
                                onClick={addSelectedModelToProvider}
                                title="添加当前下拉框选中的模型到 Provider"
                              >
                                <AppIcon name="plus" size={14} />
                              </button>
                            </div>
                          )}
                        </div>
                        <div className="ai-settings-model-right">
                          <span className="ai-settings-model-right-title">{t.aiSettingsConfiguredModelsTitle}</span>
                          {configuredModels.length > 0 ? (
                            <>
                              <div className="ai-settings-model-tags">
                                {configuredModels.map((modelName) => {
                                  const isActive = aiDraft.model === modelName
                                  return (
                                    <span
                                      key={modelName}
                                      className={`ai-settings-model-tag ${isActive ? 'is-active' : ''}`}
                                      onClick={() => patchAiDraft({ model: modelName })}
                                      title={isActive ? '当前生效模型' : `点击作为当前生效模型 (${modelName})`}
                                    >
                                      <span className="ai-settings-model-tag-text">{modelName}</span>
                                      <button
                                        type="button"
                                        className="ai-settings-model-tag-remove"
                                        onClick={(e) => {
                                          e.stopPropagation()
                                          removeConfiguredModel(modelName)
                                        }}
                                        title="从此 Provider 移除该模型"
                                      >
                                        <AppIcon name="close" size={10} />
                                      </button>
                                    </span>
                                  )
                                })}
                              </div>
                              <small className="ai-settings-model-default-hint">{t.aiSettingsModelDefaultHint}</small>
                            </>
                          ) : (
                            <div className="ai-settings-model-empty-hint">{t.aiSettingsModelEmptyHint}</div>
                          )}
                        </div>
                      </div>
                    </label>
                    <label className="ai-settings-form-span-two">
                      <span>{t.aiSettingsEndpoint}</span>
                      <input
                        placeholder={t.aiSettingsEndpointPlaceholder}
                        value={aiDraft.baseUrl}
                        onChange={(event) => patchAiDraft({ baseUrl: event.target.value })}
                      />
                    </label>
                    <div className="ai-settings-form-span-two ai-settings-form-field">
                      <div className="ai-settings-api-key-header">
                        <span>{t.aiSettingsApiKey}</span>
                        {selectedAiProvider?.hasApiKey && !clearAiApiKey ? (
                          <button
                            type="button"
                            className="ai-settings-clear-key-btn"
                            onClick={(event) => {
                              event.preventDefault()
                              event.stopPropagation()
                              setClearAiApiKey(true)
                              setAiApiKey('')
                            }}
                            title={t.aiSettingsClearApiKeyHint}
                          >
                            <AppIcon name="trash" size={13} />
                            {t.aiSettingsClearApiKey}
                          </button>
                        ) : clearAiApiKey ? (
                          <span className="ai-settings-key-cleared-tag">
                            保存时将清除 Key
                            <button
                              type="button"
                              className="ai-settings-undo-clear-btn"
                              onClick={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                                setClearAiApiKey(false)
                              }}
                            >
                              撤销
                            </button>
                          </span>
                        ) : null}
                      </div>
                      <input
                        autoComplete="off"
                        disabled={clearAiApiKey}
                        placeholder={
                          clearAiApiKey
                            ? '已标记保存时清除已保存的 API Key'
                            : selectedAiProvider?.hasApiKey
                              ? t.aiSettingsApiKeyReplacePlaceholder
                              : t.aiSettingsApiKeyPlaceholder
                        }
                        type="password"
                        value={aiApiKey}
                        onChange={(event) => {
                          setAiApiKey(event.target.value)
                          setClearAiApiKey(false)
                        }}
                      />
                    </div>
                  </div>

                  <div className="ai-settings-toggle-list">
                    <label className="ai-settings-toggle-row ssh-checkbox">
                      <input
                        checked={aiDraft.enabled}
                        type="checkbox"
                        onChange={(event) => patchAiDraft({ enabled: event.target.checked })}
                      />
                      <span>
                        <strong>{t.aiSettingsEnabled}</strong>
                        <small>{t.aiSettingsEnabledHint}</small>
                      </span>
                    </label>
                    <label className="ai-settings-toggle-row ssh-checkbox">
                      <input
                        checked={aiDraft.isDefault}
                        type="checkbox"
                        onChange={(event) => patchAiDraft({ isDefault: event.target.checked })}
                      />
                      <span>
                        <strong>{t.aiSettingsDefaultProvider}</strong>
                        <small>{t.aiSettingsDefaultProviderHint}</small>
                      </span>
                    </label>
                    <label className="ai-settings-toggle-row ssh-checkbox">
                      <input
                        checked={aiDraft.allowNoAuth}
                        type="checkbox"
                        onChange={(event) => patchAiDraft({ allowNoAuth: event.target.checked })}
                      />
                      <span>
                        <strong>{t.aiSettingsAllowNoAuth}</strong>
                        <small>{t.aiSettingsAllowNoAuthHint}</small>
                      </span>
                    </label>
                    <label className="ai-settings-toggle-row ssh-checkbox">
                      <input
                        checked={aiDraft.allowInsecureHttp}
                        type="checkbox"
                        onChange={(event) => patchAiDraft({ allowInsecureHttp: event.target.checked })}
                      />
                      <span>
                        <strong>{t.aiSettingsAllowInsecureHttp}</strong>
                        <small>{t.aiSettingsAllowInsecureHttpHint}</small>
                      </span>
                    </label>
                  </div>
                </fieldset>

                {aiDraft.allowInsecureHttp ? (
                  <p className="ai-settings-warning" role="alert">
                    {t.aiSettingsInsecureHttpWarning}
                  </p>
                ) : null}

                <div className="ai-settings-privacy-card">
                  <AppIcon name="key" size={16} />
                  <div>
                    <strong>{t.aiSettingsPrivacyTitle}</strong>
                    <p>{t.aiSettingsPrivacyDescription}</p>
                  </div>
                </div>

                <div className="ai-settings-footer">
                  <small className={aiMessage ? 'ai-settings-operation-message' : undefined} role="status">
                    {aiMessage || t.aiSettingsConnectionTestHint}
                  </small>
                  <div className="ai-settings-footer-actions">
                    {aiDraft.id ? (
                      <button
                        className="ai-settings-danger-button"
                        disabled={!desktopApi || aiOperation !== null}
                        type="button"
                        onClick={() => setShowDeleteAiProviderConfirm(true)}
                      >
                        <AppIcon name="trash" size={14} />
                        {aiOperation === 'delete' ? t.aiSettingsDeleting : t.aiSettingsDelete}
                      </button>
                    ) : null}
                    <button
                      className="ai-settings-secondary-button"
                      disabled={!desktopApi || aiOperation !== null}
                      type="button"
                      onClick={() => void testAiProvider()}
                    >
                      <AppIcon name="flash" size={14} />
                      {aiOperation === 'test' ? t.aiSettingsTesting : t.aiSettingsTestConnection}
                    </button>
                    <button
                      className="primary-button compact"
                      disabled={!desktopApi || aiOperation !== null}
                      type="button"
                      onClick={() => void saveAiProvider()}
                    >
                      <AppIcon name="disk" size={14} />
                      {aiOperation === 'save' ? t.aiSettingsSaving : t.aiSettingsSave}
                    </button>
                  </div>
                </div>
                {showDeleteAiProviderConfirm ? (
                  <ConfirmActionDialog
                    confirmLabel={t.delete}
                    confirmVariant="danger"
                    description={`确定要删除 Provider "${aiDraft.name || aiDraft.id}" 吗？删除后不可恢复。`}
                    isSubmitting={aiOperation === 'delete'}
                    onClose={() => {
                      if (aiOperation !== 'delete') {
                        setShowDeleteAiProviderConfirm(false)
                      }
                    }}
                    onConfirm={() => void deleteAiProvider()}
                    title="删除 Provider 确认"
                  />
                ) : null}
              </section>
            </div>
          ) : null}

          {activeTab === 'agent' ? (
            <div className="settings-panel settings-agent-mcp-panel">
              <section className="settings-section">
                <h3>{t.agentMcpSettings}</h3>
                <p className="settings-tools-hint">{t.agentMcpDescription}</p>

                <div className="agent-mcp-subtabs" role="tablist" aria-label={t.agentMcpSubTabs}>
                  <button
                    id="agent-mcp-tab-mcp"
                    aria-controls="agent-mcp-panel-mcp"
                    aria-selected={agentSubTab === 'mcp'}
                    className={`agent-mcp-subtab-button ${agentSubTab === 'mcp' ? 'active' : ''}`}
                    role="tab"
                    type="button"
                    onClick={() => setAgentSubTab('mcp')}
                  >
                    {t.agentMcpTabMcp}
                  </button>
                  <button
                    id="agent-mcp-tab-cli"
                    aria-controls="agent-mcp-panel-cli"
                    aria-selected={agentSubTab === 'cli'}
                    className={`agent-mcp-subtab-button ${agentSubTab === 'cli' ? 'active' : ''}`}
                    role="tab"
                    type="button"
                    onClick={() => setAgentSubTab('cli')}
                  >
                    {t.agentMcpTabCli}
                  </button>
                </div>

                {agentSubTab === 'mcp' ? (
                  <div
                    id="agent-mcp-panel-mcp"
                    className="agent-mcp-tabpanel"
                    role="tabpanel"
                    aria-labelledby="agent-mcp-tab-mcp"
                  >
                    <div className="agent-mcp-runtime-card">
                      <span className="agent-mcp-runtime-icon">
                        <AppIcon name="terminal-file" size={17} strokeWidth={2} />
                      </span>
                      <div>
                        <strong>{t.agentMcpRuntimeTitle}</strong>
                        <p>{t.agentMcpRuntimeDescription}</p>
                      </div>
                    </div>

                    <div className="agent-mcp-form">
                      <label>
                        <span>{t.agentMcpConnectionScope}</span>
                        <DropdownSelect
                          className="agent-mcp-select"
                          disabled={!desktopApi || mcpAgentOperation !== null}
                          options={[
                            { value: 'all-saved-connections', label: t.agentMcpScopeAll },
                            { value: 'active-session', label: t.agentMcpScopeActive },
                            {
                              value: 'default-connection',
                              label: t.agentMcpScopeDefault,
                              disabled: !mcpAgentProfiles.length
                            }
                          ]}
                          value={mcpAgentPreferences.connectionScope}
                          onChange={(value) => {
                            saveMcpAgentPreferences({
                              connectionScope: value as McpAgentPreferences['connectionScope'],
                              ...(value === 'default-connection' && !mcpAgentPreferences.defaultProfileId
                                ? { defaultProfileId: mcpAgentProfiles[0]?.id }
                                : {})
                            })
                          }}
                        />
                        <small>
                          {mcpAgentPreferences.connectionScope === 'all-saved-connections'
                            ? t.agentMcpScopeAllHint
                            : mcpAgentPreferences.connectionScope === 'active-session'
                              ? t.agentMcpScopeActiveHint
                              : t.agentMcpScopeDefaultHint}
                        </small>
                      </label>

                      {mcpAgentPreferences.connectionScope === 'default-connection' ? (
                        <label>
                          <span>{t.agentMcpDefaultConnection}</span>
                          <DropdownSelect
                            className="agent-mcp-select"
                            disabled={!desktopApi || mcpAgentOperation !== null || !mcpAgentProfiles.length}
                            options={mcpAgentProfiles.map((profile) => ({
                              value: profile.id,
                              label: `${profile.name || profile.host}:${profile.port}`
                            }))}
                            placeholder={t.agentMcpDefaultConnectionPlaceholder}
                            value={mcpAgentPreferences.defaultProfileId ?? ''}
                            onChange={(value) => saveMcpAgentPreferences({ defaultProfileId: value })}
                          />
                          {!mcpAgentProfiles.length ? <small>{t.agentMcpNoProfiles}</small> : null}
                        </label>
                      ) : null}

                      <label>
                        <span>{t.agentMcpOperationPolicy}</span>
                        <DropdownSelect
                          className="agent-mcp-select"
                          disabled={!desktopApi || mcpAgentOperation !== null}
                          options={[
                            { value: 'read-only', label: t.agentMcpReadOnly },
                            { value: 'approved-operations', label: t.agentMcpApprovedOperations }
                          ]}
                          value={mcpAgentPreferences.operationPolicy}
                          onChange={(value) =>
                            saveMcpAgentPreferences({
                              operationPolicy: value as McpAgentPreferences['operationPolicy']
                            })
                          }
                        />
                        <small>
                          {mcpAgentPreferences.operationPolicy === 'read-only'
                            ? t.agentMcpReadOnlyHint
                            : t.agentMcpApprovedOperationsHint}
                        </small>
                      </label>
                    </div>

                    <div className="agent-mcp-clients" aria-busy={mcpAgentOperation === 'load'}>
                      <h4>{t.agentMcpClients}</h4>
                      {mcpAgentSetup?.clients.map((client) => (
                        <article key={client.id} className="agent-mcp-client-card">
                          <div className="agent-mcp-client-heading">
                            <div>
                              <strong>{client.label}</strong>
                              <small>
                                {client.available ? t.agentMcpClientAvailable : t.agentMcpClientUnavailable}
                              </small>
                            </div>
                            <span className={`agent-mcp-client-status ${client.available ? 'is-available' : ''}`}>
                              {client.command}
                            </span>
                          </div>
                          <div className="agent-mcp-registration">
                            <code>{client.registrationCommand}</code>
                            <button
                              aria-label={t.agentMcpRegistration}
                              className="copy-icon-button agent-mcp-copy-button"
                              disabled={!desktopApi}
                              title={t.agentMcpRegistration}
                              type="button"
                              onClick={() => copyMcpAgentRegistrationCommand(client.registrationCommand)}
                            >
                              <AppIcon name="copy" size={14} strokeWidth={2} />
                            </button>
                          </div>
                          <div className="agent-mcp-client-actions">
                            <small className="agent-mcp-registration-hint">{t.agentMcpRegistrationDescription}</small>
                            <button
                              className="settings-secondary-button agent-mcp-launch-button"
                              disabled={!client.available || !onLaunchLocalAgent}
                              title={client.available ? t.agentMcpLaunchDescription : t.agentMcpClientUnavailable}
                              type="button"
                              onClick={() => launchMcpAgentInLocalTerminal(client)}
                            >
                              <AppIcon name="terminal-file" size={14} strokeWidth={2} />
                              {t.agentMcpLaunch}
                            </button>
                          </div>
                        </article>
                      ))}
                    </div>

                    <div className="agent-mcp-keep-open">
                      <AppIcon name="server" size={15} />
                      <div>
                        <strong>{t.agentMcpKeepOpenTitle}</strong>
                        <p>{t.agentMcpKeepOpenDescription}</p>
                      </div>
                    </div>
                    {mcpAgentMessage ? <p className="agent-mcp-operation-message">{mcpAgentMessage}</p> : null}
                  </div>
                ) : (
                  <div
                    id="agent-mcp-panel-cli"
                    className="agent-mcp-tabpanel"
                    role="tabpanel"
                    aria-labelledby="agent-mcp-tab-cli"
                  >
                    {mcpAgentSetup ? (
                      <div className="agent-mcp-direct-cli-card">
                        <div>
                          <strong>{t.agentMcpDirectCliTitle}</strong>
                          <p>{t.agentMcpDirectCliDescription}</p>
                        </div>
                        <div className="agent-mcp-direct-cli-commands">
                          <div className="agent-mcp-direct-cli-command">
                            <small>{t.agentMcpDirectCliPath}</small>
                            <div className="agent-mcp-registration">
                              <code>{mcpAgentSetup.filetermCommand} --help</code>
                              <button
                                aria-label={t.agentMcpDirectCliCopy}
                                className="copy-icon-button agent-mcp-copy-button"
                                disabled={!desktopApi}
                                title={t.agentMcpDirectCliCopy}
                                type="button"
                                onClick={() =>
                                  copyMcpAgentCommand(
                                    `${mcpAgentSetup.filetermCommand} --help`,
                                    t.agentMcpDirectCliCopied
                                  )
                                }
                              >
                                <AppIcon name="copy" size={14} strokeWidth={2} />
                              </button>
                            </div>
                          </div>
                        </div>
                      </div>
                    ) : null}
                    {mcpAgentMessage ? <p className="agent-mcp-operation-message">{mcpAgentMessage}</p> : null}
                  </div>
                )}
              </section>
            </div>
          ) : null}

          {activeTab === 'connections' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.connectionDefaults}</h3>
                <p className="settings-tools-hint">{t.connectionDefaultsHint}</p>
                <fieldset
                  className="settings-connection-defaults"
                  disabled={!desktopApi || isSavingConnectionDefaults}
                  style={{ border: 0, margin: 0, padding: 0 }}
                >
                  <div className="advanced-toggle-list">
                    <div className="advanced-toggle-row">
                      <label className="ssh-checkbox advanced-toggle-label">
                        <input
                          checked={connectionDefaults.useEmptyPassword}
                          onChange={(event) => setConnectionDefault('useEmptyPassword', event.target.checked)}
                          type="checkbox"
                        />
                        <span className="advanced-toggle-name">{t.useEmptyPassword}</span>
                      </label>
                      <p className="advanced-toggle-hint">{t.useEmptyPasswordHint}</p>
                    </div>
                    <div className="advanced-toggle-row">
                      <label className="ssh-checkbox advanced-toggle-label">
                        <input
                          checked={connectionDefaults.enableExecChannel}
                          onChange={(event) => setConnectionDefault('enableExecChannel', event.target.checked)}
                          type="checkbox"
                        />
                        <span className="advanced-toggle-name">{t.enableExecChannel}</span>
                      </label>
                      <p className="advanced-toggle-hint">{t.enableExecChannelHint}</p>
                    </div>
                    <div className="advanced-toggle-row">
                      <label className="ssh-checkbox advanced-toggle-label">
                        <input
                          checked={connectionDefaults.enableResourceMonitoring}
                          onChange={(event) => setConnectionDefault('enableResourceMonitoring', event.target.checked)}
                          type="checkbox"
                        />
                        <span className="advanced-toggle-name">{t.resourceMonitoring}</span>
                      </label>
                      <p className="advanced-toggle-hint">{t.resourceMonitoringDescription}</p>
                      <label className="resource-monitoring-interval">
                        <span>{t.resourceMonitoringInterval}</span>
                        <DropdownSelect
                          className="resource-monitoring-interval__select"
                          disabled={!connectionDefaults.enableResourceMonitoring}
                          options={[
                            { value: '1', label: t.resourceMonitoringEverySecond },
                            { value: '5', label: t.resourceMonitoringEvery5Seconds },
                            { value: '15', label: t.resourceMonitoringEvery15Seconds },
                            { value: '30', label: t.resourceMonitoringEvery30Seconds },
                            { value: '60', label: t.resourceMonitoringEvery60Seconds }
                          ]}
                          value={String(connectionDefaults.resourceMonitoringIntervalSeconds)}
                          onChange={(value) =>
                            setConnectionDefault(
                              'resourceMonitoringIntervalSeconds',
                              Number(value) as SshConnectionDefaults['resourceMonitoringIntervalSeconds']
                            )
                          }
                        />
                      </label>
                    </div>
                    <div className="advanced-toggle-row">
                      <label className="ssh-checkbox advanced-toggle-label">
                        <input
                          checked={connectionDefaults.legacyAlgorithms}
                          onChange={(event) => setConnectionDefault('legacyAlgorithms', event.target.checked)}
                          type="checkbox"
                        />
                        <span className="advanced-toggle-name">{t.legacyAlgorithms}</span>
                      </label>
                      <p className="advanced-toggle-hint">{t.legacyAlgorithmsHint}</p>
                    </div>
                  </div>
                  <div className="reconnect-mode-group">
                    <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
                    <div className="advanced-toggle-list">
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={connectionDefaults.reconnectMode === 'none'}
                            name="global-reconnect-mode"
                            onChange={() => setConnectionDefault('reconnectMode', 'none')}
                            type="radio"
                          />
                          <span className="advanced-toggle-name">{t.reconnectNone}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={connectionDefaults.reconnectMode === 'enter'}
                            name="global-reconnect-mode"
                            onChange={() => setConnectionDefault('reconnectMode', 'enter')}
                            type="radio"
                          />
                          <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
                      </div>
                      <div className="advanced-toggle-row">
                        <label className="ssh-checkbox advanced-toggle-label">
                          <input
                            checked={connectionDefaults.reconnectMode === 'auto'}
                            name="global-reconnect-mode"
                            onChange={() => setConnectionDefault('reconnectMode', 'auto')}
                            type="radio"
                          />
                          <span className="advanced-toggle-name">{t.autoReconnect}</span>
                        </label>
                        <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                      </div>
                    </div>
                  </div>
                </fieldset>
                {connectionDefaultsError ? <p className="modal-error">{connectionDefaultsError}</p> : null}
              </section>
            </div>
          ) : null}

          {activeTab === 'interface' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.appearanceTheme}</h3>
                <div aria-label={t.themeSelection} className="theme-options-grid" role="group">
                  <button
                    aria-pressed={themeConfig.variant === 'light'}
                    className={`theme-card light ${themeConfig.variant === 'light' ? 'active' : ''}`}
                    onClick={() => switchThemeVariant('light')}
                    type="button"
                  >
                    <div aria-hidden="true" className="theme-card-preview">
                      <div className="preview-header">
                        <span className="dot dot-close" />
                        <span className="dot dot-min" />
                        <span className="dot dot-max" />
                      </div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span className="theme-card-label">{t.themeLight}</span>
                  </button>
                  <button
                    aria-pressed={themeConfig.variant === 'dark'}
                    className={`theme-card dark ${themeConfig.variant === 'dark' ? 'active' : ''}`}
                    onClick={() => switchThemeVariant('dark')}
                    type="button"
                  >
                    <div aria-hidden="true" className="theme-card-preview">
                      <div className="preview-header">
                        <span className="dot dot-close" />
                        <span className="dot dot-min" />
                        <span className="dot dot-max" />
                      </div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span className="theme-card-label">{t.themeDark}</span>
                  </button>
                </div>
              </section>

              <section className="settings-section theme-config-section">
                <div className="theme-config-heading">
                  <div>
                    <h3>{t.themeCustomization}</h3>
                    <p className="settings-tools-hint">{t.themeCustomizationHint}</p>
                  </div>
                  <div className="theme-config-action-group">
                    <div className="theme-config-actions">
                      <button
                        className="flat-button compact theme-config-action-button"
                        disabled={themeConfigOperation !== null}
                        onClick={() => void importThemeConfig()}
                        type="button"
                      >
                        <AppIcon name="download" size={14} />
                        {themeConfigOperation === 'import' ? t.themeWorking : t.themeImport}
                      </button>
                      <button
                        className="flat-button compact theme-config-action-button"
                        disabled={themeConfigOperation !== null}
                        onClick={() => void copyThemeConfig()}
                        type="button"
                      >
                        <AppIcon name="copy" size={14} />
                        {themeConfigOperation === 'copy' ? t.themeWorking : t.themeCopy}
                      </button>
                    </div>
                    {themeConfigMessage ? (
                      <span aria-live="polite" className="theme-config-action-status">
                        {themeConfigMessage}
                      </span>
                    ) : null}
                  </div>
                </div>

                <div className="theme-config-toolbar">
                  <div className="theme-config-preset-control">
                    <span className="theme-config-label">{t.themePreset}</span>
                    <DropdownSelect
                      ariaLabel={t.themePreset}
                      className="theme-config-select"
                      onChange={applyThemePreset}
                      options={[
                        ...THEME_PRESETS.map((preset) => ({ value: preset.id, label: t[preset.labelKey] })),
                        ...customThemes.map((savedTheme) => ({
                          value: `saved:${savedTheme.id}`,
                          label: savedTheme.name
                        })),
                        { value: 'custom', label: t.themePresetCustom }
                      ]}
                      value={themePresetValue}
                    />
                  </div>
                  <div className="theme-config-name-control">
                    <span className="theme-config-label">{t.themeCustomName}</span>
                    <input
                      aria-label={t.themeCustomName}
                      className="theme-config-name-input"
                      maxLength={128}
                      onChange={(event) => setCustomThemeName(event.target.value)}
                      placeholder={t.themeCustomNamePlaceholder}
                      value={customThemeName}
                    />
                  </div>
                  <div className="theme-config-actions-control">
                    <button
                      className="primary-button compact theme-config-save-button"
                      onClick={saveCustomTheme}
                      type="button"
                    >
                      <AppIcon name={editingSavedTheme ? 'check' : 'edit'} size={14} />
                      {editingSavedTheme ? t.themeUpdate : t.themeSave}
                    </button>
                    {selectedSavedTheme ? (
                      <button
                        className="theme-config-danger-button"
                        onClick={() => setShowDeleteThemeConfirm(true)}
                        type="button"
                      >
                        <AppIcon name="trash" size={14} />
                        {t.themeDelete}
                      </button>
                    ) : null}
                  </div>
                  <div
                    className="theme-config-preview"
                    style={{
                      backgroundColor: themeConfig.theme.surface,
                      borderColor: 'var(--border-light)',
                      color: themeConfig.theme.ink
                    }}
                  >
                    <div className="theme-config-preview-title">
                      <span style={{ color: themeConfig.theme.accent }}>●</span>
                      <span>{themePresetLabel}</span>
                    </div>
                    <code className="theme-config-preview-code">
                      <span style={{ color: themeConfig.theme.semanticColors.keyword }}>const</span>{' '}
                      <span style={{ color: themeConfig.theme.semanticColors.skill }}>theme</span>
                      {' = '}
                      <span style={{ color: themeConfig.theme.accent }}>{themePresetCode}</span>
                    </code>
                  </div>
                </div>

                <div className="theme-config-color-groups">
                  <section className="theme-config-color-group">
                    <div className="theme-config-section-heading">
                      <h4>{t.themeBaseColors}</h4>
                      <span>{t.themeBaseColorsHint}</span>
                    </div>
                    <div className="theme-config-fields">
                      <ThemeColorField
                        label={t.themePrimaryColor}
                        onChange={(value) => updateThemeBody({ accent: value })}
                        value={themeConfig.theme.accent}
                      />
                      <ThemeColorField
                        label={t.themeSecondaryColor}
                        onChange={(value) => updateThemeSemanticColors({ secondary: value })}
                        value={themeConfig.theme.semanticColors.secondary}
                      />
                      <ThemeColorField
                        label={t.themeSurfaceColor}
                        onChange={(value) => updateThemeBody({ surface: value })}
                        value={themeConfig.theme.surface}
                      />
                      <ThemeColorField
                        label={t.themeSurfaceSecondaryColor}
                        onChange={(value) => updateThemeBody({ surfaceSecondary: value })}
                        value={themeConfig.theme.surfaceSecondary}
                      />
                      <ThemeColorField
                        label={t.themeSurfaceElevatedColor}
                        onChange={(value) => updateThemeBody({ surfaceElevated: value })}
                        value={themeConfig.theme.surfaceElevated}
                      />
                      <ThemeColorField
                        label={t.themeTextPrimaryColor}
                        onChange={(value) => updateThemeBody({ ink: value })}
                        value={themeConfig.theme.ink}
                      />
                      <ThemeColorField
                        label={t.themeTextSecondaryColor}
                        onChange={(value) => updateThemeSemanticColors({ textSecondary: value })}
                        value={themeConfig.theme.semanticColors.textSecondary}
                      />
                    </div>
                  </section>

                  <section className="theme-config-color-group">
                    <div className="theme-config-section-heading">
                      <h4>{t.themeStatusColors}</h4>
                      <span>{t.themeStatusColorsHint}</span>
                    </div>
                    <div className="theme-config-fields">
                      <ThemeColorField
                        label={t.themeInfoColor}
                        onChange={(value) => updateThemeSemanticColors({ info: value })}
                        value={themeConfig.theme.semanticColors.info}
                      />
                      <ThemeColorField
                        label={t.themeWarningColor}
                        onChange={(value) => updateThemeSemanticColors({ warning: value })}
                        value={themeConfig.theme.semanticColors.warning}
                      />
                      <ThemeColorField
                        label={t.themeErrorColor}
                        onChange={(value) => updateThemeSemanticColors({ error: value })}
                        value={themeConfig.theme.semanticColors.error}
                      />
                      <ThemeColorField
                        label={t.themeSuccessColor}
                        onChange={(value) => updateThemeSemanticColors({ success: value })}
                        value={themeConfig.theme.semanticColors.success}
                      />
                    </div>
                  </section>
                </div>

                <div className="theme-config-font-grid">
                  <div className="theme-config-control">
                    <span className="theme-config-label">{t.themeUiFont}</span>
                    <DropdownSelect
                      ariaLabel={t.themeUiFont}
                      className="theme-config-select"
                      onChange={(value) => updateThemeFonts({ ui: value || null })}
                      options={[
                        { value: '', label: t.themeSystemDefault },
                        { value: 'Inter', label: 'Inter' },
                        { value: 'SF Pro Text', label: 'SF Pro Text' },
                        { value: 'Noto Sans SC', label: 'Noto Sans SC' }
                      ]}
                      value={themeConfig.theme.fonts.ui ?? ''}
                    />
                  </div>
                  <div className="theme-config-control">
                    <span className="theme-config-label">{t.themeCodeFont}</span>
                    <DropdownSelect
                      ariaLabel={t.themeCodeFont}
                      className="theme-config-select"
                      onChange={(value) => updateThemeFonts({ code: value || null })}
                      options={[
                        { value: '', label: t.themeSystemDefault },
                        { value: 'JetBrains Mono', label: 'JetBrains Mono' },
                        { value: 'SF Mono', label: 'SF Mono' },
                        { value: 'Cascadia Code', label: 'Cascadia Code' }
                      ]}
                      value={themeConfig.theme.fonts.code ?? ''}
                    />
                  </div>
                </div>

                <div className="theme-config-checkbox-row">
                  <label className="theme-config-switch">
                    <div className="theme-config-switch-info">
                      <span className="theme-config-switch-title">{t.themeOpaqueWindows}</span>
                      <span className="theme-config-switch-hint">{t.themeOpaqueWindowsHint}</span>
                    </div>
                    <input
                      checked={themeConfig.theme.opaqueWindows}
                      onChange={(event) => updateThemeBody({ opaqueWindows: event.target.checked })}
                      type="checkbox"
                    />
                    <span aria-hidden="true" className="theme-config-switch-track" />
                  </label>
                </div>

                <details className="theme-config-subsection theme-advanced-section">
                  <summary className="theme-config-section-summary">
                    <span className="theme-config-section-summary-copy">
                      <strong>{t.themeSemanticColors}</strong>
                      <span>{t.themeAdvancedHint}</span>
                    </span>
                  </summary>
                  <div className="theme-config-fields">
                    <ThemeColorField
                      label={t.themeDiffAdded}
                      onChange={(value) => updateThemeSemanticColors({ diffAdded: value })}
                      value={themeConfig.theme.semanticColors.diffAdded}
                    />
                    <ThemeColorField
                      label={t.themeDiffRemoved}
                      onChange={(value) => updateThemeSemanticColors({ diffRemoved: value })}
                      value={themeConfig.theme.semanticColors.diffRemoved}
                    />
                    <ThemeColorField
                      label={t.themeSkillColor}
                      onChange={(value) => updateThemeSemanticColors({ skill: value })}
                      value={themeConfig.theme.semanticColors.skill}
                    />
                    <ThemeColorField
                      label={t.themeKeywordColor}
                      onChange={(value) => updateThemeSemanticColors({ keyword: value })}
                      value={themeConfig.theme.semanticColors.keyword}
                    />
                  </div>
                </details>

                <div className="theme-config-subsection">
                  <h4>{t.themeTerminalColors}</h4>
                  <div className="theme-config-fields">
                    <ThemeColorField
                      label={t.themeTerminalBackground}
                      onChange={(value) => updateTerminalTheme({ background: value })}
                      value={themeConfig.theme.terminal.background}
                    />
                    <ThemeColorField
                      label={t.themeTerminalForeground}
                      onChange={(value) => updateTerminalTheme({ foreground: value })}
                      value={themeConfig.theme.terminal.foreground}
                    />
                    <ThemeColorField
                      label={t.themeTerminalCursor}
                      onChange={(value) => updateTerminalTheme({ cursor: value })}
                      value={themeConfig.theme.terminal.cursor}
                    />
                    <ThemeColorField
                      label={t.themeTerminalCursorAccent}
                      onChange={(value) => updateTerminalTheme({ cursorAccent: value })}
                      value={themeConfig.theme.terminal.cursorAccent}
                    />
                    <ThemeColorField
                      label={t.themeTerminalSelection}
                      onChange={(value) => updateTerminalTheme({ selectionBackground: value })}
                      value={themeConfig.theme.terminal.selectionBackground}
                    />
                    <ThemeColorField
                      label={t.themeTerminalSelectionText}
                      onChange={(value) => updateTerminalTheme({ selectionForeground: value })}
                      value={themeConfig.theme.terminal.selectionForeground}
                    />
                  </div>
                  <div className="theme-config-fields theme-config-search-fields">
                    <ThemeColorField
                      label={t.themeSearchMatch}
                      onChange={(value) => updateTerminalSearchColors({ matchBackground: value })}
                      value={themeConfig.theme.terminal.search.matchBackground}
                    />
                    <ThemeColorField
                      label={t.themeSearchActiveMatch}
                      onChange={(value) => updateTerminalSearchColors({ activeMatchBackground: value })}
                      value={themeConfig.theme.terminal.search.activeMatchBackground}
                    />
                    <ThemeColorField
                      label={t.themeSearchActiveText}
                      onChange={(value) => updateTerminalSearchColors({ activeMatchText: value })}
                      value={themeConfig.theme.terminal.search.activeMatchText}
                    />
                    <ThemeColorField
                      label={t.themeSearchBorder}
                      onChange={(value) => updateTerminalSearchColors({ activeMatchBorder: value })}
                      value={themeConfig.theme.terminal.search.activeMatchBorder}
                    />
                  </div>
                </div>

                <details className="theme-advanced-section theme-terminal-ansi-details">
                  <summary className="theme-config-section-summary">
                    <span className="theme-config-section-summary-copy">
                      <strong>{t.themeAnsiNormal}</strong>
                      <span>{t.themeAnsiHint}</span>
                    </span>
                    <span className="theme-config-section-summary-count">16</span>
                  </summary>
                  <div className="theme-ansi-grid">
                    {ANSI_COLOR_NAMES.slice(0, 8).map((name) => (
                      <ThemeColorField
                        key={name}
                        label={ANSI_COLOR_LABELS[name]}
                        onChange={(value) => updateTerminalAnsiColor(name, value)}
                        value={themeConfig.theme.terminal.ansi[name]}
                      />
                    ))}
                  </div>
                  <h4>{t.themeAnsiBright}</h4>
                  <div className="theme-ansi-grid">
                    {ANSI_COLOR_NAMES.slice(8).map((name) => (
                      <ThemeColorField
                        key={name}
                        label={ANSI_COLOR_LABELS[name]}
                        onChange={(value) => updateTerminalAnsiColor(name, value)}
                        value={themeConfig.theme.terminal.ansi[name]}
                      />
                    ))}
                  </div>
                </details>

                {showDeleteThemeConfirm && selectedSavedTheme ? (
                  <ConfirmActionDialog
                    confirmLabel={t.delete}
                    confirmVariant="danger"
                    description={t.themeDeleteConfirmDescription.replace('{name}', selectedSavedTheme.name)}
                    onClose={() => setShowDeleteThemeConfirm(false)}
                    onConfirm={deleteCustomTheme}
                    title={t.themeDeleteConfirmTitle}
                  />
                ) : null}
              </section>

              <section className="settings-section">
                <h3>{t.terminalDisplaySettings}</h3>
                <p className="settings-tools-hint">{t.terminalDisplaySettingsHint}</p>
                <div className="overview-preference-list">
                  <label className="overview-preference-row">
                    <span className="overview-preference-copy">
                      <strong>{t.lockTerminalZoom}</strong>
                      <p>{t.lockTerminalZoomHint}</p>
                    </span>
                    <span className="command-toggle overview-preference-toggle">
                      <input
                        checked={terminalZoomLocked}
                        disabled={!desktopApi || isSavingTerminalZoomPreference}
                        onChange={(event) => setTerminalZoomLockPreference(event.target.checked)}
                        type="checkbox"
                      />
                    </span>
                  </label>
                </div>
                {terminalZoomPreferenceError ? <p className="modal-error">{terminalZoomPreferenceError}</p> : null}
              </section>

              <section className="settings-section">
                <h3>{t.overviewContentSettings}</h3>
                <p className="settings-tools-hint">{t.overviewContentSettingsHint}</p>
                <div className="overview-preference-list">
                  {draggingOverviewSection && overviewSectionOrder[0] ? (
                    <div
                      aria-hidden="true"
                      className="overview-preference-top-drop-zone"
                      data-fileterm-sort-id={overviewSectionOrder[0]}
                      data-fileterm-sort-kind="overview-section-top"
                    />
                  ) : null}
                  {overviewSectionOrder.map((sectionId) => {
                    const isDragging = draggingOverviewSection === sectionId
                    const isDragOver = dragOverOverviewSection === sectionId
                    const sectionMeta = overviewSectionMeta[sectionId]
                    const checked =
                      sectionId === 'stats'
                        ? overviewShowStats
                        : sectionId === 'recent'
                          ? overviewShowRecent
                          : sectionId === 'allConnections'
                            ? overviewShowAllConnections
                            : overviewShowQuickActions

                    return (
                      <label
                        className={`overview-preference-row ${isDragging ? 'dragging' : ''} ${managerDropClass(isDragOver, overviewDragPosition)}`}
                        data-fileterm-sort-id={sectionId}
                        data-fileterm-sort-kind="overview-section"
                        draggable={false}
                        key={sectionId}
                        onClick={(event) => {
                          if (suppressOverviewCardClickRef.current) {
                            event.preventDefault()
                            event.stopPropagation()
                          }
                        }}
                        onPointerDown={(event) => {
                          if (!isSavingOverviewPreference && !targetsNestedManagerControl(event)) {
                            handleOverviewPointerDown(event, sectionId)
                          }
                        }}
                      >
                        <span
                          aria-label={t.overviewDragToReorder}
                          className="material-symbols-outlined overview-preference-drag-handle"
                          title={t.overviewDragToReorder}
                        >
                          drag_indicator
                        </span>
                        <span className="overview-preference-copy">
                          <strong>{sectionMeta.title}</strong>
                          <p>{sectionMeta.hint}</p>
                        </span>
                        <span className="command-toggle overview-preference-toggle">
                          <input
                            checked={checked}
                            disabled={!desktopApi || isSavingOverviewPreference}
                            onChange={(event) => {
                              if (sectionId === 'stats') setOverviewShowStatsPreference(event.target.checked)
                              else if (sectionId === 'recent') setOverviewShowRecentPreference(event.target.checked)
                              else if (sectionId === 'allConnections') {
                                setOverviewShowAllConnectionsPreference(event.target.checked)
                              } else {
                                setOverviewShowQuickActionsPreference(event.target.checked)
                              }
                            }}
                            type="checkbox"
                          />
                        </span>
                      </label>
                    )
                  })}
                </div>
                {overviewPreferenceError ? <p className="modal-error">{overviewPreferenceError}</p> : null}
              </section>
            </div>
          ) : null}

          {activeTab === 'tools' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.managerToolsShortcut}</h3>
                <p className="settings-tools-hint">{managerToolsHint}</p>
                <div className="tools-shortcuts-grid">
                  <div className="tool-shortcut-card">
                    <span className="material-symbols-outlined tool-card-icon">settings_ethernet</span>
                    <div className="tool-card-details">
                      <strong>{t.connectionManager}</strong>
                      <p>{t.settingsConnectionManagerDescription}</p>
                      <button className="primary-button compact" onClick={onOpenConnectionManager} type="button">
                        {managerToolsActionLabel}
                      </button>
                    </div>
                  </div>
                  <div className="tool-shortcut-card">
                    <span className="material-symbols-outlined tool-card-icon">terminal</span>
                    <div className="tool-card-details">
                      <strong>{t.commandManager}</strong>
                      <p>{t.settingsCommandManagerDescription}</p>
                      <button className="primary-button compact" onClick={onOpenCommandManager} type="button">
                        {managerToolsActionLabel}
                      </button>
                    </div>
                  </div>
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'sync' && syncConfig ? (
            <div className="settings-panel">
              <div className="sync-subtabs">
                <button
                  type="button"
                  className={`sync-subtab-button ${syncSubTab === 'webdav' ? 'active' : ''}`}
                  onClick={() => setSyncSubTab('webdav')}
                >
                  <AppIcon name="cloud" size={15} />
                  <span>WebDAV</span>
                </button>
                <button
                  type="button"
                  className={`sync-subtab-button ${syncSubTab === 's3' ? 'active' : ''}`}
                  onClick={() => setSyncSubTab('s3')}
                >
                  <AppIcon name="database" size={15} />
                  <span>S3</span>
                </button>
              </div>

              {syncSubTab === 'webdav' && (
                <section className="settings-section">
                  <h3>{t.webdavConfigSync}</h3>
                  <p className="settings-tools-hint">{t.webdavConfigSyncDescription}</p>
                  <fieldset disabled={syncOperation !== null} style={{ border: 0, margin: 0, padding: 0 }}>
                    <div className="webdav-sync-form">
                      <label>
                        <span>{t.webdavUrl}</span>
                        <input
                          value={syncConfig.url}
                          placeholder="https://dav.example.com/remote.php/dav/files/me"
                          onChange={(event) => setSyncConfig({ ...syncConfig, url: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavRemoteFile}</span>
                        <input
                          value={syncConfig.remotePath}
                          placeholder="fileterm-connections.json"
                          onChange={(event) => setSyncConfig({ ...syncConfig, remotePath: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavUsername}</span>
                        <input
                          value={syncConfig.username ?? ''}
                          onChange={(event) => setSyncConfig({ ...syncConfig, username: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.webdavPassword}</span>
                        <input
                          type="password"
                          autoComplete="new-password"
                          value={syncPassword}
                          placeholder={t.webdavPasswordPlaceholder}
                          onChange={(event) => setSyncPassword(event.target.value)}
                        />
                      </label>
                      <div className="webdav-sync-options">
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={syncConfig.enabled}
                            onChange={(event) => setSyncConfig({ ...syncConfig, enabled: event.target.checked })}
                          />
                          {t.enableWebdavSync}
                        </label>
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={syncConfig.allowInsecureTls === true}
                            onChange={(event) =>
                              setSyncConfig({ ...syncConfig, allowInsecureTls: event.target.checked })
                            }
                          />
                          {t.allowInsecureHttp}
                        </label>
                      </div>
                    </div>

                    <div className="sync-config-actions-row">
                      <div className="sync-config-primary-buttons">
                        <button
                          className="primary-button compact"
                          disabled={syncOperation !== null}
                          type="button"
                          onClick={() => {
                            if (!desktopApi) return
                            void runSyncOperation('save', async () => {
                              const config = await desktopApi.saveWebDavSyncConfig({
                                ...syncConfig,
                                ...(syncPassword ? { password: syncPassword } : {})
                              })
                              setSyncConfig(config)
                              setSyncPassword('')
                              setSyncMessage(t.syncConfigSaved)
                            })
                          }}
                        >
                          {syncOperation === 'save' ? <span aria-hidden="true" className="button-spinner" /> : null}
                          <span>{t.save}</span>
                        </button>
                        <button
                          className="flat-button compact"
                          disabled={syncOperation !== null}
                          type="button"
                          onClick={() => {
                            if (!desktopApi) return
                            void runSyncOperation('test', async () => {
                              const result = await desktopApi.testWebDavSync()
                              setSyncMessage(result.message)
                            })
                          }}
                        >
                          {syncOperation === 'test' ? <span aria-hidden="true" className="button-spinner" /> : null}
                          <span>{t.webdavTestConnection}</span>
                        </button>
                      </div>

                      {syncConfig.lastSyncedAt ? (
                        <div className="sync-last-synced-badge">
                          <AppIcon name="history" size={13} />
                          <span>
                            {t.lastSync.replace('{time}', new Date(syncConfig.lastSyncedAt).toLocaleString())}
                          </span>
                        </div>
                      ) : null}
                    </div>

                    {syncMessage ? (
                      <div className="sync-feedback-banner">
                        <span>{syncMessage}</span>
                      </div>
                    ) : null}

                    <div className="sync-operations-card">
                      <div className="sync-operations-card-header">
                        <div className="sync-operations-card-title">
                          <AppIcon name="refresh" size={15} />
                          <h4>{t.manualSyncTitle}</h4>
                        </div>
                        <span className="sync-operations-card-subtitle">{t.manualSyncDescription}</span>
                      </div>

                      <div className="sync-operations-grid">
                        <div className="sync-op-box">
                          <div className="sync-op-box-header">
                            <div className="sync-op-badge upload">
                              <AppIcon name="upload" size={14} />
                            </div>
                            <div>
                              <span className="sync-op-name">{t.syncUploadTitle}</span>
                              <p className="sync-op-help">{t.syncUploadHint}</p>
                            </div>
                          </div>
                          <div className="sync-op-controls-row">
                            <div className="sync-op-select-field">
                              <span className="sync-op-label">{t.syncUploadMode}</span>
                              <DropdownSelect
                                value={backupUploadMode}
                                options={[
                                  { value: 'overwrite-cloud', label: t.syncUploadOverwriteCloud },
                                  { value: 'merge-cloud', label: t.syncUploadMergeCloud }
                                ]}
                                onChange={(value) => setBackupUploadMode(value as BackupUploadMode)}
                              />
                            </div>
                            <button
                              className="flat-button compact sync-op-btn"
                              disabled={!syncConfig.enabled || syncOperation !== null}
                              type="button"
                              onClick={() => {
                                if (!desktopApi) return
                                void runSyncOperation('upload', async () => {
                                  const result = await desktopApi.uploadWebDavSync(backupUploadMode)
                                  setSyncMessage(result.message)
                                })
                              }}
                            >
                              {syncOperation === 'upload' ? (
                                <span aria-hidden="true" className="button-spinner" />
                              ) : (
                                <AppIcon name="upload" size={13} />
                              )}
                              <span>{t.syncUpload}</span>
                            </button>
                          </div>
                        </div>

                        <div className="sync-op-box">
                          <div className="sync-op-box-header">
                            <div className="sync-op-badge download">
                              <AppIcon name="download" size={14} />
                            </div>
                            <div>
                              <span className="sync-op-name">{t.syncDownloadTitle}</span>
                              <p className="sync-op-help">{t.syncDownloadHint}</p>
                            </div>
                          </div>
                          <div className="sync-op-controls-row">
                            <div className="sync-op-select-field">
                              <span className="sync-op-label">{t.syncDownloadMode}</span>
                              <DropdownSelect
                                value={backupDownloadMode}
                                options={[
                                  { value: 'merge-local', label: t.syncDownloadMergeLocal },
                                  { value: 'overwrite-local', label: t.syncDownloadOverwriteLocal }
                                ]}
                                onChange={(value) => setBackupDownloadMode(value as BackupDownloadMode)}
                              />
                            </div>
                            <button
                              className="flat-button compact sync-op-btn"
                              disabled={!syncConfig.enabled || syncOperation !== null}
                              type="button"
                              onClick={() => {
                                if (!desktopApi) return
                                void runSyncOperation('download', async () => {
                                  const result = await desktopApi.downloadWebDavSync(backupDownloadMode)
                                  setSyncMessage(result.message)
                                })
                              }}
                            >
                              {syncOperation === 'download' ? (
                                <span aria-hidden="true" className="button-spinner" />
                              ) : (
                                <AppIcon name="download" size={13} />
                              )}
                              <span>{t.syncDownload}</span>
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </fieldset>
                </section>
              )}

              {syncSubTab === 's3' && s3Config && (
                <section className="settings-section">
                  <h3>{t.s3Backup}</h3>
                  <p className="settings-tools-hint">{t.s3BackupDescription}</p>
                  <fieldset disabled={syncOperation !== null} style={{ border: 0, margin: 0, padding: 0 }}>
                    <div className="webdav-sync-form">
                      <label>
                        <span>{t.s3Provider}</span>
                        <DropdownSelect
                          value={s3Config.provider}
                          options={[
                            { value: 'cloudflare-r2', label: t.s3ProviderCloudflareR2 },
                            { value: 'bitiful-s4', label: t.s3ProviderBitifulS4 },
                            { value: 'custom', label: t.s3ProviderCustom }
                          ]}
                          onChange={(provider) => {
                            const isR2 = provider === 'cloudflare-r2'
                            const isBitiful = provider === 'bitiful-s4'
                            setS3Config({
                              ...s3Config,
                              provider: isR2 ? 'cloudflare-r2' : isBitiful ? 'bitiful-s4' : 'custom',
                              endpoint: isBitiful ? 'https://s3.bitiful.net' : s3Config.endpoint,
                              region: isR2
                                ? 'auto'
                                : isBitiful
                                  ? 'cn-east-1'
                                  : s3Config.region === 'auto'
                                    ? 'us-east-1'
                                    : s3Config.region,
                              pathStyleAccessEnabled: isR2 ? true : isBitiful ? false : s3Config.pathStyleAccessEnabled
                            })
                          }}
                        />
                      </label>
                      <label>
                        <span>{t.s3Endpoint}</span>
                        <input
                          readOnly={s3Config.provider === 'bitiful-s4'}
                          value={s3Config.endpoint}
                          placeholder={
                            s3Config.provider === 'bitiful-s4'
                              ? 'https://s3.bitiful.net'
                              : 'https://<account-id>.r2.cloudflarestorage.com'
                          }
                          onChange={(event) => setS3Config({ ...s3Config, endpoint: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3Region}</span>
                        <input
                          disabled={s3Config.provider === 'cloudflare-r2' || s3Config.provider === 'bitiful-s4'}
                          value={s3Config.region}
                          placeholder="auto"
                          onChange={(event) => setS3Config({ ...s3Config, region: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3Bucket}</span>
                        <input
                          value={s3Config.bucket}
                          onChange={(event) => setS3Config({ ...s3Config, bucket: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3ObjectKey}</span>
                        <input
                          value={s3Config.remotePath}
                          placeholder="fileterm/connections.json"
                          onChange={(event) => setS3Config({ ...s3Config, remotePath: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3AccessKeyId}</span>
                        <input
                          autoComplete="off"
                          value={s3Config.accessKeyId ?? ''}
                          onChange={(event) => setS3Config({ ...s3Config, accessKeyId: event.target.value })}
                        />
                      </label>
                      <label>
                        <span>{t.s3SecretAccessKey}</span>
                        <input
                          type="password"
                          autoComplete="new-password"
                          value={s3SecretAccessKey}
                          placeholder={s3Config.hasSavedSecret ? t.s3SecretAccessKeyPlaceholder : undefined}
                          onChange={(event) => setS3SecretAccessKey(event.target.value)}
                        />
                      </label>
                      <div className="webdav-sync-options">
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            checked={s3Config.enabled}
                            onChange={(event) => setS3Config({ ...s3Config, enabled: event.target.checked })}
                          />
                          {t.enableS3Backup}
                        </label>
                        <label className="webdav-checkbox ssh-checkbox">
                          <input
                            type="checkbox"
                            disabled={s3Config.provider === 'cloudflare-r2' || s3Config.provider === 'bitiful-s4'}
                            checked={s3Config.pathStyleAccessEnabled}
                            onChange={(event) =>
                              setS3Config({ ...s3Config, pathStyleAccessEnabled: event.target.checked })
                            }
                          />
                          {t.s3PathStyle}
                        </label>
                      </div>
                    </div>

                    <div className="sync-config-actions-row">
                      <div className="sync-config-primary-buttons">
                        <button
                          className="primary-button compact"
                          type="button"
                          onClick={() => {
                            if (!desktopApi) return
                            void runSyncOperation('s3-save', async () => {
                              const config = await desktopApi.saveS3BackupConfig({
                                ...s3Config,
                                ...(s3SecretAccessKey ? { secretAccessKey: s3SecretAccessKey } : {})
                              })
                              setS3Config(config)
                              setS3SecretAccessKey('')
                              setS3Message(t.s3BackupSaved)
                            })
                          }}
                        >
                          {syncOperation === 's3-save' ? <span aria-hidden="true" className="button-spinner" /> : null}
                          <span>{t.save}</span>
                        </button>
                        <button
                          className="flat-button compact"
                          disabled={syncOperation !== null}
                          type="button"
                          onClick={() => {
                            if (!desktopApi) return
                            void runSyncOperation('s3-test', async () => {
                              const result = await desktopApi.testS3Backup()
                              setS3Message(result.message)
                            })
                          }}
                        >
                          {syncOperation === 's3-test' ? <span aria-hidden="true" className="button-spinner" /> : null}
                          <span>{t.s3TestConnection}</span>
                        </button>
                      </div>

                      {s3Config.lastSyncedAt ? (
                        <div className="sync-last-synced-badge">
                          <AppIcon name="history" size={13} />
                          <span>{t.lastSync.replace('{time}', new Date(s3Config.lastSyncedAt).toLocaleString())}</span>
                        </div>
                      ) : null}
                    </div>

                    {s3Message ? (
                      <div className="sync-feedback-banner">
                        <span>{s3Message}</span>
                      </div>
                    ) : null}

                    <div className="sync-operations-card">
                      <div className="sync-operations-card-header">
                        <div className="sync-operations-card-title">
                          <AppIcon name="refresh" size={15} />
                          <h4>{t.manualSyncTitle}</h4>
                        </div>
                        <span className="sync-operations-card-subtitle">{t.manualSyncDescription}</span>
                      </div>

                      <div className="sync-operations-grid">
                        <div className="sync-op-box">
                          <div className="sync-op-box-header">
                            <div className="sync-op-badge upload">
                              <AppIcon name="upload" size={14} />
                            </div>
                            <div>
                              <span className="sync-op-name">{t.syncUploadTitle}</span>
                              <p className="sync-op-help">{t.syncUploadHint}</p>
                            </div>
                          </div>
                          <div className="sync-op-controls-row">
                            <div className="sync-op-select-field">
                              <span className="sync-op-label">{t.syncUploadMode}</span>
                              <DropdownSelect
                                value={backupUploadMode}
                                options={[
                                  { value: 'overwrite-cloud', label: t.syncUploadOverwriteCloud },
                                  { value: 'merge-cloud', label: t.syncUploadMergeCloud }
                                ]}
                                onChange={(value) => setBackupUploadMode(value as BackupUploadMode)}
                              />
                            </div>
                            <button
                              className="flat-button compact sync-op-btn"
                              disabled={!s3Config.enabled || syncOperation !== null}
                              type="button"
                              onClick={() => {
                                if (!desktopApi) return
                                void runSyncOperation('s3-upload', async () => {
                                  const result = await desktopApi.uploadS3Backup(backupUploadMode)
                                  setS3Message(result.message)
                                })
                              }}
                            >
                              {syncOperation === 's3-upload' ? (
                                <span aria-hidden="true" className="button-spinner" />
                              ) : (
                                <AppIcon name="upload" size={13} />
                              )}
                              <span>{t.syncUpload}</span>
                            </button>
                          </div>
                        </div>

                        <div className="sync-op-box">
                          <div className="sync-op-box-header">
                            <div className="sync-op-badge download">
                              <AppIcon name="download" size={14} />
                            </div>
                            <div>
                              <span className="sync-op-name">{t.syncDownloadTitle}</span>
                              <p className="sync-op-help">{t.syncDownloadHint}</p>
                            </div>
                          </div>
                          <div className="sync-op-controls-row">
                            <div className="sync-op-select-field">
                              <span className="sync-op-label">{t.syncDownloadMode}</span>
                              <DropdownSelect
                                value={backupDownloadMode}
                                options={[
                                  { value: 'merge-local', label: t.syncDownloadMergeLocal },
                                  { value: 'overwrite-local', label: t.syncDownloadOverwriteLocal }
                                ]}
                                onChange={(value) => setBackupDownloadMode(value as BackupDownloadMode)}
                              />
                            </div>
                            <button
                              className="flat-button compact sync-op-btn"
                              disabled={!s3Config.enabled || syncOperation !== null}
                              type="button"
                              onClick={() => {
                                if (!desktopApi) return
                                void runSyncOperation('s3-download', async () => {
                                  const result = await desktopApi.downloadS3Backup(backupDownloadMode)
                                  setS3Message(result.message)
                                })
                              }}
                            >
                              {syncOperation === 's3-download' ? (
                                <span aria-hidden="true" className="button-spinner" />
                              ) : (
                                <AppIcon name="download" size={13} />
                              )}
                              <span>{t.syncDownload}</span>
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </fieldset>
                </section>
              )}
            </div>
          ) : null}

          {activeTab === 'sync' && !syncConfig ? (
            <div className="settings-panel">
              <section aria-busy="true" className="settings-section">
                <h3>{t.webdavConfigSync}</h3>
                <p className="settings-tools-hint">
                  <span aria-hidden="true" className="button-spinner" /> {t.loadingSyncConfig}
                </p>
                {syncMessage ? <p className="modal-error">{syncMessage}</p> : null}
              </section>
            </div>
          ) : null}

          {activeTab === 'updates' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.appUpdates}</h3>
                <div className="update-check-preference">
                  <div>
                    <strong>{t.updateCheckPreference}</strong>
                    <p>{t.updateCheckPreferenceHint}</p>
                  </div>
                  <DropdownSelect
                    className="update-check-preference-select"
                    disabled={!desktopApi || isSavingUpdatePreference}
                    onChange={(value) => setUpdateCheckPreference(value === 'auto')}
                    value={autoCheckUpdates ? 'auto' : 'manual'}
                    options={[
                      { value: 'auto', label: t.autoCheckUpdates },
                      { value: 'manual', label: t.doNotAutoUpdate }
                    ]}
                  />
                </div>
                <div className="update-check-preference">
                  <div>
                    <strong>{t.updateChannel}</strong>
                    <p>{t.updateChannelHint}</p>
                  </div>
                  <DropdownSelect
                    className="update-check-preference-select"
                    disabled={!desktopApi || isSavingUpdatePreference}
                    onChange={(value) => setUpdateChannelPreference(value === 'beta' ? 'beta' : 'stable')}
                    value={updateChannel}
                    options={[
                      { value: 'stable', label: t.stableChannel },
                      { value: 'beta', label: t.betaChannel }
                    ]}
                  />
                </div>
                {updatePreferenceError ? <p className="modal-error">{updatePreferenceError}</p> : null}
                <div className="update-status-card" aria-live="polite">
                  <div>
                    <strong>{t.updateStatus}</strong>
                    <p>{getUpdateStatusLabel(updateStatus, t, autoCheckUpdates)}</p>
                  </div>
                  <span className={`update-status-indicator ${updateStatus?.state ?? 'idle'}`} />
                </div>
                {updateStatus?.state === 'downloading' ? (
                  <div className="update-progress" aria-label={t.updateDownloading}>
                    <span style={{ width: `${updateStatus.progress ?? 0}%` }} />
                  </div>
                ) : null}
                <div className="settings-update-actions">
                  {updateStatus?.state === 'available' ? (
                    <button
                      className="primary-button compact"
                      onClick={() => {
                        if (updateStatus.updateMode === 'release-page') {
                          void desktopApi?.openExternalUrl(
                            updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases'
                          )
                        } else {
                          void desktopApi?.downloadUpdate()
                        }
                      }}
                      type="button"
                    >
                      {updateStatus.updateMode === 'release-page' ? t.openReleasePage : t.downloadUpdate}
                    </button>
                  ) : null}
                  {updateStatus?.state === 'downloaded' ? (
                    <button
                      className="primary-button compact"
                      onClick={() => void desktopApi?.installUpdate()}
                      type="button"
                    >
                      {t.restartToUpdate}
                    </button>
                  ) : null}
                  {updateStatus?.state !== 'downloading' && updateStatus?.state !== 'downloaded' ? (
                    <button
                      className="flat-button compact"
                      disabled={updateStatus?.state === 'checking' || updateStatus?.state === 'unsupported'}
                      onClick={() => void desktopApi?.checkForUpdates()}
                      type="button"
                    >
                      {updateStatus?.state === 'checking' ? t.checkingForUpdates : t.checkForUpdates}
                    </button>
                  ) : null}
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'system' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.aboutAppInfo}</h3>
                <div className="about-info-list">
                  <div className="about-info-item">
                    <span className="info-label">{t.versionLabel}</span>
                    <span className="info-value">v{desktopApi?.appVersion ?? '—'}</span>
                  </div>
                  <div className="about-info-item">
                    <span className="info-label">{desktopApi?.runtimeName ?? '—'}</span>
                    <span className="info-value">v{desktopApi?.runtimeVersion ?? '—'}</span>
                  </div>
                  <div className="about-info-item">
                    <span className="info-label">{t.environmentInfo}</span>
                    <span className="info-value">{platformLabel}</span>
                  </div>
                </div>
              </section>

              <section className="settings-section">
                <h3>{t.systemLogsInfo}</h3>
                <div className="logs-shortcut-card">
                  <p>{t.settingsLogsDescription}</p>
                  <button className="flat-button compact" onClick={onOpenLogsDirectory} type="button">
                    <span
                      className="material-symbols-outlined"
                      style={{ fontSize: '14px', marginRight: '4px', verticalAlign: 'middle' }}
                    >
                      folder_open
                    </span>
                    {t.openLogsDirectory}
                  </button>
                </div>
              </section>
            </div>
          ) : null}

          {activeTab === 'language' ? (
            <div className="settings-panel">
              <section className="settings-section">
                <h3>{t.languageSelection}</h3>
                <div className="language-selector-row">
                  <button
                    className={`lang-card ${locale === 'zhCN' ? 'active' : ''}`}
                    onClick={() => onSetLocale('zhCN')}
                    type="button"
                  >
                    {t.languageZhCN}
                  </button>
                  <button
                    className={`lang-card ${locale === 'enUS' ? 'active' : ''}`}
                    onClick={() => onSetLocale('enUS')}
                    type="button"
                  >
                    {t.languageEnglish}
                  </button>
                </div>
              </section>
            </div>
          ) : null}
        </main>
      </div>
    </div>
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

function getUpdateStatusLabel(status: AppUpdateStatus | null, labels: typeof t, autoCheckUpdates: boolean) {
  if (!status) return autoCheckUpdates ? labels.updateStatusIdle : labels.updateStatusManual
  if (status.state === 'available') {
    const label = status.updateMode === 'release-page' ? labels.updateAvailableManual : labels.updateAvailable
    return label.replace('{version}', status.availableVersion ?? '—')
  }
  if (status.state === 'downloaded') return labels.updateDownloaded.replace('{version}', status.availableVersion ?? '—')
  if (status.state === 'downloading')
    return labels.updateDownloading.replace('{progress}', String(status.progress ?? 0))
  if (status.state === 'not-available') return labels.updateNotAvailable
  if (status.state === 'checking') return labels.checkingForUpdates
  if (status.state === 'error') return `${labels.updateFailed}: ${status.message ?? '—'}`
  if (status.state === 'unsupported') return labels.updateUnsupported
  return labels.updateStatusIdle
}
