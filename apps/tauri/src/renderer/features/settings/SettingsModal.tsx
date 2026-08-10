import { useEffect, useRef, useState } from 'react'
import {
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  DEFAULT_OVERVIEW_SECTION_ORDER,
  type AppUpdateStatus,
  type AiProviderDraft,
  type AiProviderKind,
  type AiProviderSummary,
  type OverviewSectionId,
  type S3BackupConfig,
  type SshConnectionDefaults,
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

type SettingsTab = 'ai' | 'connections' | 'interface' | 'sync' | 'tools' | 'updates' | 'system' | 'language'

function sameOverviewSectionOrder(left: OverviewSectionId[], right: OverviewSectionId[]) {
  return left.length === right.length && left.every((sectionId, index) => sectionId === right[index])
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

function aiProviderToDraft(provider: AiProviderSummary): AiProviderDraft {
  return {
    id: provider.id,
    name: provider.name,
    kind: provider.kind,
    baseUrl: provider.baseUrl,
    model: provider.model,
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
  onSetTheme,
  locale,
  onSetLocale,
  onOpenCommandManager,
  onOpenConnectionManager,
  onOpenLogsDirectory,
  onClose,
  initialTab = 'interface',
  standalone = false,
  inline = false
}: {
  theme: 'default-dark' | 'default-light'
  onSetTheme(value: 'default-dark' | 'default-light'): void
  locale: 'zhCN' | 'enUS'
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenCommandManager(): void
  onOpenConnectionManager(): void
  onOpenLogsDirectory(): void
  onClose(): void
  initialTab?: SettingsTab
  standalone?: boolean
  inline?: boolean
}) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab)
  const [syncSubTab, setSyncSubTab] = useState<'webdav' | 's3'>('webdav')
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(true)
  const [isSavingUpdatePreference, setIsSavingUpdatePreference] = useState(false)
  const [updatePreferenceError, setUpdatePreferenceError] = useState<string | null>(null)
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
  const [aiProviders, setAiProviders] = useState<AiProviderSummary[]>([])
  const [aiDraft, setAiDraft] = useState<AiProviderDraft>(() => createAiProviderDraft())
  // Candidate model IDs carried by the currently applied preset. Cleared when
  // the user picks an already-configured provider (no preset bound). Stored
  // outside AiProviderDraft to keep the data-layer type free of UI-only state.
  const [aiModelChoices, setAiModelChoices] = useState<string[]>([])
  const [isCustomModel, setIsCustomModel] = useState(false)
  const [aiApiKey, setAiApiKey] = useState('')
  const [clearAiApiKey, setClearAiApiKey] = useState(false)
  const [aiMessage, setAiMessage] = useState<string | null>(null)
  const [aiOperation, setAiOperation] = useState<'load' | 'save' | 'test' | 'delete' | null>(null)
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
        setAiDraft((current) => {
          const selected = current.id ? providers.find((provider) => provider.id === current.id) : undefined
          const fallback = providers.find((provider) => provider.isDefault) ?? providers[0]
          const nextProvider = selected ?? fallback
          return nextProvider ? aiProviderToDraft(nextProvider) : createAiProviderDraft(true)
        })
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
    setAiDraft(provider ? aiProviderToDraft(provider) : createAiProviderDraft(aiProviders.length === 0))
    setAiModelChoices([])
    setIsCustomModel(false)
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
      model: preset.draft.model,
      allowNoAuth: preset.draft.allowNoAuth,
      allowInsecureHttp: preset.draft.allowInsecureHttp
    }))
    setAiModelChoices(preset.draft.models ?? [])
    setIsCustomModel(false)
    setAiMessage(null)
  }

  const aiProviderInput = () => {
    const secrets = clearAiApiKey ? { apiKey: null } : aiApiKey.trim() ? { apiKey: aiApiKey } : undefined
    return {
      provider: aiDraft,
      ...(secrets ? { secrets } : {})
    }
  }

  const saveAiProvider = async () => {
    if (!desktopApi || aiOperation) return
    setAiOperation('save')
    setAiMessage(null)
    try {
      const saved = await desktopApi.saveAiProvider(aiProviderInput())
      const providers = await desktopApi.listAiProviders()
      setAiProviders(providers)
      const selected = providers.find((provider) => provider.id === saved.id) ?? saved
      setAiDraft(aiProviderToDraft(selected))
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage(t.aiSettingsSaveSucceeded)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setAiApiKey('')
      setClearAiApiKey(false)
      setAiOperation(null)
    }
  }

  const testAiProvider = async () => {
    if (!desktopApi || aiOperation) return
    setAiOperation('test')
    setAiMessage(null)
    try {
      const result = await desktopApi.testAiProvider(aiProviderInput())
      setAiMessage(result.message)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setAiOperation(null)
    }
  }

  const deleteAiProvider = async () => {
    if (!desktopApi || !aiDraft.id || aiOperation) return

    setAiOperation('delete')
    setAiMessage(null)
    try {
      const providers = await desktopApi.deleteAiProvider(aiDraft.id)
      setAiProviders(providers)
      const fallback = providers.find((provider) => provider.isDefault) ?? providers[0]
      setAiDraft(fallback ? aiProviderToDraft(fallback) : createAiProviderDraft(true))
      setAiApiKey('')
      setClearAiApiKey(false)
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage(t.aiSettingsDeleteSucceeded)
      setShowDeleteAiProviderConfirm(false)
    } catch (error) {
      setAiMessage(error instanceof Error ? error.message : String(error))
    } finally {
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
                      // Value is controlled by parent; DropdownSelect re-renders
                      // with `__none__` so the same preset can be re-applied.
                    }}
                  />
                  <p className="ai-settings-preset-hint">{t.aiSettingsPresetHint}</p>
                </div>

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
                    <label>
                      <span>{t.aiSettingsModel}</span>
                      {aiModelChoices.length > 0 && !isCustomModel && aiModelChoices.includes(aiDraft.model) ? (
                        <DropdownSelect
                          className="ai-settings-model-select"
                          disabled={!desktopApi || aiOperation !== null}
                          value={aiDraft.model}
                          options={[
                            ...aiModelChoices.map((model) => ({ value: model, label: model })),
                            { value: '__custom__', label: '自定义模型...' }
                          ]}
                          onChange={(value) => {
                            if (value === '__custom__') {
                              setIsCustomModel(true)
                            } else {
                              patchAiDraft({ model: value })
                            }
                          }}
                        />
                      ) : (
                        <div className="ai-settings-model-row">
                          <input
                            className="ai-settings-model-input"
                            placeholder={t.aiSettingsModelPlaceholder}
                            value={aiDraft.model}
                            onChange={(event) => patchAiDraft({ model: event.target.value })}
                          />
                          {aiModelChoices.length > 0 && (
                            <button
                              type="button"
                              className="button button-secondary"
                              onClick={() => {
                                setIsCustomModel(false)
                                if (!aiModelChoices.includes(aiDraft.model)) {
                                  patchAiDraft({ model: aiModelChoices[0] })
                                }
                              }}
                              title="从预设模型列表中选择"
                            >
                              从预设选择
                            </button>
                          )}
                        </div>
                      )}
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
                    {aiMessage ?? t.aiSettingsConnectionTestHint}
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
                <div className="theme-options-grid">
                  <button
                    className={`theme-card dark ${theme === 'default-dark' ? 'active' : ''}`}
                    onClick={() => onSetTheme('default-dark')}
                    type="button"
                  >
                    <div className="theme-card-preview">
                      <div className="preview-header"></div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span>
                      {t.theme}: {t.defaultDark}
                    </span>
                  </button>
                  <button
                    className={`theme-card light ${theme === 'default-light' ? 'active' : ''}`}
                    onClick={() => onSetTheme('default-light')}
                    type="button"
                  >
                    <div className="theme-card-preview">
                      <div className="preview-header"></div>
                      <div className="preview-body">
                        <div className="preview-sidebar"></div>
                        <div className="preview-content"></div>
                      </div>
                    </div>
                    <span>
                      {t.theme}: {t.defaultLight}
                    </span>
                  </button>
                </div>
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
                  <span className="material-symbols-outlined">cloud_sync</span>
                  <span>WebDAV</span>
                </button>
                <button
                  type="button"
                  className={`sync-subtab-button ${syncSubTab === 's3' ? 'active' : ''}`}
                  onClick={() => setSyncSubTab('s3')}
                >
                  <span className="material-symbols-outlined">database</span>
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
                    <div className="settings-update-actions webdav-sync-actions">
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
                      <button
                        className="flat-button compact"
                        disabled={!syncConfig.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('upload', async () => {
                            const result = await desktopApi.uploadWebDavSync()
                            setSyncMessage(result.message)
                          })
                        }}
                      >
                        {syncOperation === 'upload' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncUpload}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!syncConfig.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('download', async () => {
                            const result = await desktopApi.downloadWebDavSync()
                            setSyncMessage(result.message)
                          })
                        }}
                      >
                        {syncOperation === 'download' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncDownload}</span>
                      </button>
                    </div>
                  </fieldset>
                  {syncConfig.lastSyncedAt ? (
                    <p className="settings-tools-hint">
                      {t.lastSync.replace('{time}', new Date(syncConfig.lastSyncedAt).toLocaleString())}
                    </p>
                  ) : null}
                  {syncMessage ? <p className="settings-tools-hint">{syncMessage}</p> : null}
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
                    <div className="settings-update-actions webdav-sync-actions">
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
                      <button
                        className="flat-button compact"
                        disabled={!s3Config.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-upload', async () => {
                            const result = await desktopApi.uploadS3Backup()
                            setS3Message(result.message)
                          })
                        }}
                      >
                        {syncOperation === 's3-upload' ? <span aria-hidden="true" className="button-spinner" /> : null}
                        <span>{t.syncUpload}</span>
                      </button>
                      <button
                        className="flat-button compact"
                        disabled={!s3Config.enabled || syncOperation !== null}
                        type="button"
                        onClick={() => {
                          if (!desktopApi) return
                          void runSyncOperation('s3-download', async () => {
                            const result = await desktopApi.downloadS3Backup()
                            setS3Message(result.message)
                          })
                        }}
                      >
                        {syncOperation === 's3-download' ? (
                          <span aria-hidden="true" className="button-spinner" />
                        ) : null}
                        <span>{t.syncDownload}</span>
                      </button>
                    </div>
                  </fieldset>
                  {s3Config.lastSyncedAt ? (
                    <p className="settings-tools-hint">
                      {t.lastSync.replace('{time}', new Date(s3Config.lastSyncedAt).toLocaleString())}
                    </p>
                  ) : null}
                  {s3Message ? <p className="settings-tools-hint">{s3Message}</p> : null}
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
