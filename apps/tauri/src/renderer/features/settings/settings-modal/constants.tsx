import { useEffect, useState } from 'react'
import {
  createCodexThemeConfig,
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type AiProviderDraft,
  type AiProviderKind,
  type AiProviderSummary,
  type ConnectionProfile,
  type LocalTerminalPlatform,
  type LocalTerminalShellOption,
  type OverviewSectionId,
  type SavedTheme,
  type TerminalAnsiColorName,
  type ThemeConfig
} from '@fileterm/core'
import { getSavedThemeConfig } from '../../../app/theme-config'
import { type LocaleMessages } from '../../../i18n'
import { type AppIconName } from '../../common/app-icon'

export type SettingsTab =
  | 'ai'
  | 'agent'
  | 'connections'
  | 'interface'
  | 'local-terminal'
  | 'security'
  | 'sync'
  | 'tools'
  | 'updates'
  | 'system'
  | 'language'

export type SyncFeedback = {
  kind: 'success' | 'error'
  message: string
}

export type AiFeedback = {
  kind: 'success' | 'error'
  message: string
}

export type SettingsSidebarItem = {
  tab: SettingsTab
  labelKey: keyof LocaleMessages
  materialIcon?: string
  appIcon?: AppIconName
}

export const SETTINGS_SIDEBAR_ITEMS: SettingsSidebarItem[] = [
  { tab: 'interface', labelKey: 'interfaceSettings', materialIcon: 'palette' },
  { tab: 'local-terminal', labelKey: 'localTerminalSettings', appIcon: 'terminal-file' },
  { tab: 'ai', labelKey: 'aiSettings', materialIcon: 'auto_awesome' },
  { tab: 'agent', labelKey: 'agentMcpSettings', appIcon: 'terminal-file' },
  { tab: 'connections', labelKey: 'connectionDefaults', materialIcon: 'settings_ethernet' },
  { tab: 'security', labelKey: 'securitySettings', appIcon: 'shield-check' },
  { tab: 'sync', labelKey: 'configSync', materialIcon: 'cloud_sync' },
  { tab: 'updates', labelKey: 'appUpdates', materialIcon: 'system_update' },
  { tab: 'tools', labelKey: 'managerToolsShortcut', materialIcon: 'apps' },
  { tab: 'system', labelKey: 'systemLogsInfo', materialIcon: 'info' },
  { tab: 'language', labelKey: 'languageSidebarLabel', materialIcon: 'translate' }
]

export const SETTINGS_TAB_SEARCH_TERMS: Record<SettingsTab, string> = {
  interface: 'appearance overview theme color font ui 外观 概览 主题 颜色 字体',
  'local-terminal': 'terminal shell powershell pwsh bash zsh fish nushell 本地终端 shell',
  ai: 'ai provider model api key openai anthropic 模型 服务 密钥',
  agent: 'agent mcp cli command tool automation 代理 命令 工具',
  connections: 'connection ssh sftp ftp telnet reconnect resource monitor 连接 默认值 重连 监控',
  security: 'security session lock password backup credentials safe 安全 会话 锁屏 密码 备份 凭据',
  sync: 'sync webdav s3 backup cloud configuration 同步 备份 云端',
  updates: 'update release version beta stable 更新 版本 发布',
  tools: 'manager connection command key shortcut 管理器 连接 命令 密钥 快捷键',
  system: 'system log diagnostics platform version 系统 日志 诊断 平台 版本',
  language: 'language locale chinese english 中文 英文 语言'
}

export type ThemePresetFamily = 'fileterm' | 'codex'
export type ThemePresetVariant = ThemeConfig['variant']

export const THEME_HEX_COLOR_PATTERN = /^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i
export const THEME_CONFIG_EXPORT_PREFIX = 'fileterm-theme-v1:'
export const THEME_CONFIG_IMPORT_PREFIXES = [THEME_CONFIG_EXPORT_PREFIX, 'codex-theme-v1:'] as const
export const FILETERM_CLI_SKILL_URL = 'https://github.com/St0ff3l/fileterm/blob/main/docs/fileterm-cli.md'

export const ANSI_COLOR_NAMES: TerminalAnsiColorName[] = [
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

export const ANSI_COLOR_LABELS: Record<TerminalAnsiColorName, string> = {
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

export const LOCAL_TERMINAL_SHELL_CONFIGS: Array<{
  platform: LocalTerminalPlatform
  labelKey: 'localTerminalShellWindows' | 'localTerminalShellMacos' | 'localTerminalShellLinux'
  hintKey: 'localTerminalShellWindowsHint' | 'localTerminalShellMacosHint' | 'localTerminalShellLinuxHint'
  placeholder: string
}> = [
  {
    platform: 'win32',
    labelKey: 'localTerminalShellWindows',
    hintKey: 'localTerminalShellWindowsHint',
    placeholder: 'pwsh.exe'
  },
  {
    platform: 'darwin',
    labelKey: 'localTerminalShellMacos',
    hintKey: 'localTerminalShellMacosHint',
    placeholder: '/bin/zsh'
  },
  {
    platform: 'linux',
    labelKey: 'localTerminalShellLinux',
    hintKey: 'localTerminalShellLinuxHint',
    placeholder: '/bin/bash'
  }
]

export function localTerminalShellOptionsFor(detectedOptions: LocalTerminalShellOption[]) {
  const options = detectedOptions.map((option) => ({
    shell: option.shell,
    label: `${option.label} · ${option.path}`
  }))
  const seen = new Set<string>()
  return options
    .filter((option) => {
      if (seen.has(option.shell)) return false
      seen.add(option.shell)
      return true
    })
    .map((option) => ({ value: option.shell, label: option.label }))
}

export const THEME_PRESETS: Array<{
  id: ThemePresetFamily
  labelKey: 'themePresetFileTerm' | 'themePresetCodex'
  getConfig: (variant: ThemePresetVariant) => ThemeConfig
}> = [
  {
    id: 'fileterm',
    labelKey: 'themePresetFileTerm',
    getConfig: (variant) => createDefaultThemeConfig(variant)
  },
  {
    id: 'codex',
    labelKey: 'themePresetCodex',
    getConfig: (variant) => createCodexThemeConfig(variant)
  }
]

export function findMatchingThemePreset(themeConfig: ThemeConfig): (typeof THEME_PRESETS)[number] | undefined {
  if (!themeConfig) return undefined
  const normalizedTheme = normalizeThemeConfig(themeConfig, themeConfig.variant ?? 'dark')
  return THEME_PRESETS.find((preset) => {
    const matchesId =
      preset.id === 'fileterm'
        ? normalizedTheme.codeThemeId === 'fileterm' ||
          normalizedTheme.codeThemeId === 'fileterm-dark' ||
          normalizedTheme.codeThemeId === 'fileterm-light'
        : normalizedTheme.codeThemeId === 'codex' ||
          normalizedTheme.codeThemeId === 'codex-dark' ||
          normalizedTheme.codeThemeId === 'codex-light'
    // Preset identity is authoritative. User edits and saved/imported custom
    // themes are assigned `custom`, while legacy FileTerm/Codex color tokens
    // can legitimately differ from the current renderer defaults.
    return matchesId
  })
}

export function sameThemeConfig(left: ThemeConfig, right: ThemeConfig) {
  return JSON.stringify(left) === JSON.stringify(right)
}

export function findSavedThemeForConfig(savedThemes: SavedTheme[], themeConfig: ThemeConfig) {
  if (!themeConfig) return undefined
  const normalizedTheme = normalizeThemeConfig(themeConfig, themeConfig.variant ?? 'dark')
  return savedThemes.find((candidate) =>
    sameThemeConfig(getSavedThemeConfig(candidate, normalizedTheme.variant), normalizedTheme)
  )
}

export function themeBaseIdForConfig(themeConfig: ThemeConfig): 'fileterm' | 'codex' {
  if (themeConfig.baseThemeId) return themeConfig.baseThemeId
  return themeConfig.codeThemeId === 'codex' || themeConfig.codeThemeId.startsWith('codex-') ? 'codex' : 'fileterm'
}

export function createCustomThemeId() {
  const randomId = typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : null
  return `custom-${randomId ?? `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`}`
}

export function toColorInputValue(value: unknown) {
  if (typeof value !== 'string') return '#000000'
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

export function ThemeColorField({
  label,
  value,
  onChange
}: {
  label: string
  value: string | undefined | null
  onChange(value: string): void
}) {
  const safeValue = typeof value === 'string' ? value : '#000000'
  const [draft, setDraft] = useState(safeValue)

  useEffect(() => {
    setDraft(safeValue)
  }, [safeValue])

  return (
    <label className="theme-color-field">
      <span className="theme-color-field-label">{label}</span>
      <span className="theme-color-field-control">
        <input
          aria-label={label}
          className="theme-color-picker"
          onChange={(event) => onChange(event.target.value.toUpperCase())}
          type="color"
          value={toColorInputValue(safeValue)}
        />
        <input
          aria-label={`${label} HEX`}
          className="theme-color-text"
          onBlur={() => setDraft(safeValue)}
          onChange={(event) => {
            const nextValue = event.target.value ?? ''
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

export function clipboardUnavailableError(lastError: unknown) {
  return lastError instanceof Error ? lastError : new Error('Clipboard is unavailable')
}

export function sameOverviewSectionOrder(left: OverviewSectionId[], right: OverviewSectionId[]) {
  return left.length === right.length && left.every((sectionId, index) => sectionId === right[index])
}

export const DEFAULT_MODELS_BY_KIND: Record<AiProviderKind, string[]> = {
  'openai-compatible-chat': [
    'deepseek-v4-flash',
    'deepseek-v4-pro',
    'deepseek-v4-flash-vision-exp',
    'kimi-k3',
    'kimi-k2.7-code',
    'kimi-k2.7-code-highspeed',
    'kimi-k2.6',
    'glm-5.2',
    'glm-5.1',
    'glm-5',
    'glm-4.7',
    'doubao-seed-2-0-pro-260215',
    'doubao-seed-2-0-code-260215'
  ],
  'openai-responses': [
    'gpt-5.6',
    'gpt-5.6-sol',
    'gpt-5.6-terra',
    'gpt-5.6-luna',
    'gpt-5.5',
    'gpt-5.5-pro',
    'gpt-5.4',
    'gpt-5.4-pro',
    'gpt-5.4-mini',
    'gpt-5.3-codex',
    'gpt-5.2'
  ],
  'anthropic-messages': [
    'claude-fable-5',
    'claude-opus-5',
    'claude-opus-4-8',
    'claude-opus-4-7',
    'claude-opus-4-6',
    'claude-opus-4-5-20251101',
    'claude-sonnet-5',
    'claude-sonnet-4-6',
    'claude-sonnet-4-5-20250929',
    'claude-haiku-4-5-20251001'
  ]
}

export function createAiProviderDraft(isDefault = true): AiProviderDraft {
  return {
    name: '',
    kind: 'openai-compatible-chat',
    baseUrl: '',
    model: '',
    modelCapabilities: {},
    enabled: true,
    isDefault,
    allowNoAuth: false,
    allowInsecureHttp: false
  }
}

export type AiProviderPreset = {
  id: string
  // Stable translation key suffix, resolved via `t.aiSettingsPreset_<id>`.
  labelKey: keyof LocaleMessages
  draft: {
    name: string
    kind: AiProviderKind
    baseUrl: string
    model: string
    // When non-empty, the form offers these as suggestions while still keeping
    // manual model ID entry as the primary path.
    models?: string[]
    allowNoAuth: boolean
    allowInsecureHttp: boolean
  }
}

// Curated provider presets aligned with cc-switch's common defaults so users
// can fill the form with one click instead of looking up Base URL / model IDs.
export const AI_PROVIDER_PRESETS: AiProviderPreset[] = [
  {
    id: 'anthropic-official',
    labelKey: 'aiSettingsPreset_anthropicOfficial',
    draft: {
      name: 'Anthropic',
      kind: 'anthropic-messages',
      baseUrl: 'https://api.anthropic.com/v1',
      model: 'claude-fable-5',
      models: [
        'claude-fable-5',
        'claude-opus-5',
        'claude-opus-4-8',
        'claude-opus-4-7',
        'claude-opus-4-6',
        'claude-opus-4-5-20251101',
        'claude-sonnet-5',
        'claude-sonnet-4-6',
        'claude-sonnet-4-5-20250929',
        'claude-haiku-4-5-20251001'
      ],
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
        'gpt-5.6',
        'gpt-5.6-sol',
        'gpt-5.6-terra',
        'gpt-5.6-luna',
        'gpt-5.5',
        'gpt-5.5-pro',
        'gpt-5.4',
        'gpt-5.4-pro',
        'gpt-5.4-mini',
        'gpt-5.3-codex',
        'gpt-5.2',
        'o3'
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
      baseUrl: 'https://api.deepseek.com',
      model: 'deepseek-v4-flash',
      models: ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-v4-flash-vision-exp'],
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
      baseUrl: 'https://api.moonshot.ai/v1',
      model: 'kimi-k3',
      models: ['kimi-k3', 'kimi-k2.7-code', 'kimi-k2.7-code-highspeed', 'kimi-k2.6'],
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
      models: [
        'glm-5.2',
        'glm-5.1',
        'glm-5-turbo',
        'glm-5',
        'glm-5v-turbo',
        'glm-4.7',
        'glm-4.6',
        'glm-4.6v',
        'glm-4.6v-flash',
        'glm-4.1v-thinking-flashx',
        'glm-4.1v-thinking-flash',
        'glm-4.5-air',
        'glm-4.5-airx',
        'glm-4.5-flash',
        'glm-4v-flash',
        'glm-ocr'
      ],
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
      model: 'doubao-seed-2-0-pro-260215',
      models: [
        'doubao-seed-2-1-pro-260628',
        'doubao-seed-2-1-turbo-260628',
        'doubao-seed-2-0-pro-260215',
        'doubao-seed-2-0-lite-260215',
        'doubao-seed-2-0-mini-260215',
        'doubao-seed-2-0-code-260215'
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
      // SiliconFlow's model directory changes independently of FileTerm.
      // Keep these as suggestions only; the input above accepts any current ID.
      model: 'deepseek-ai/DeepSeek-V3.2',
      models: [
        'deepseek-ai/DeepSeek-V3.2',
        'deepseek-ai/DeepSeek-V3.1-Terminus',
        'deepseek-ai/DeepSeek-R1',
        'moonshotai/Kimi-K2.6',
        'zai-org/GLM-5.1',
        'zai-org/GLM-4.7',
        'MiniMaxAI/MiniMax-M2.5',
        'Qwen/Qwen3-VL-235B-A22B-Instruct'
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
      model: '',
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
      model: '',
      allowNoAuth: true,
      allowInsecureHttp: true
    }
  }
]

const MANUAL_CAPABILITY_PRESET_IDS = new Set(['ollama-local', 'lm-studio-local'])

/** Local and custom sources keep the detailed per-model capability editor. */
export function isAiManualCapabilityProvider(draft: Pick<AiProviderDraft, 'name' | 'baseUrl'>) {
  const normalizedName = draft.name.trim().toLowerCase()
  const normalizedBaseUrl = draft.baseUrl.trim()
  const preset = AI_PROVIDER_PRESETS.find(
    (candidate) =>
      candidate.draft.baseUrl === normalizedBaseUrl || candidate.draft.name.toLowerCase() === normalizedName
  )
  return !preset || MANUAL_CAPABILITY_PRESET_IDS.has(preset.id)
}

export function aiProviderToDraft(provider: AiProviderSummary): AiProviderDraft {
  return {
    id: provider.id,
    name: provider.name,
    kind: provider.kind,
    baseUrl: provider.baseUrl,
    model: provider.model,
    models: provider.models,
    modelCapabilities: provider.modelCapabilities,
    enabled: provider.enabled,
    isDefault: provider.isDefault,
    allowNoAuth: provider.allowNoAuth,
    allowInsecureHttp: provider.allowInsecureHttp
  }
}

export function aiProviderRequestUrlPreview(draft: AiProviderDraft) {
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

export function maskAgentProfileHost(host: string) {
  const value = host.trim()
  if (!value) return ''
  if (value.length <= 4) return `•••${value.slice(-1)}`
  return `${value.slice(0, 2)}…${value.slice(-2)}`
}

export function agentProfileTarget(profile: ConnectionProfile) {
  if (profile.type === 'serial') return profile.devicePath
  const host = maskAgentProfileHost(profile.host)
  return host ? `${host}:${profile.port}` : profile.type.toUpperCase()
}
