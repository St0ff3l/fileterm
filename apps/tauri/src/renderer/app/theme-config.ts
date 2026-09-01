import {
  createCodexThemeConfig,
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type TerminalAnsiColorName,
  type ThemeBaseId,
  type ThemeConfig,
  type ThemeVariant,
  type SavedTheme
} from '@fileterm/core'

export type ThemeMode =
  'fileterm-dark' | 'fileterm-light' | 'codex-dark' | 'codex-light' | 'default-dark' | 'default-light'

const ANSI_VARIABLE_NAMES: Array<[TerminalAnsiColorName, string]> = [
  ['black', '--terminal-black'],
  ['red', '--terminal-red'],
  ['green', '--terminal-green'],
  ['yellow', '--terminal-yellow'],
  ['blue', '--terminal-blue'],
  ['magenta', '--terminal-magenta'],
  ['cyan', '--terminal-cyan'],
  ['white', '--terminal-white'],
  ['brightBlack', '--terminal-bright-black'],
  ['brightRed', '--terminal-bright-red'],
  ['brightGreen', '--terminal-bright-green'],
  ['brightYellow', '--terminal-bright-yellow'],
  ['brightBlue', '--terminal-bright-blue'],
  ['brightMagenta', '--terminal-bright-magenta'],
  ['brightCyan', '--terminal-bright-cyan'],
  ['brightWhite', '--terminal-bright-white']
]

const appliedThemeVariableNames = new Set<string>()

export function themeVariantForMode(themeMode: ThemeMode): ThemeVariant {
  return themeMode === 'default-light' || themeMode === 'fileterm-light' || themeMode === 'codex-light'
    ? 'light'
    : 'dark'
}

export function isDarkTheme(themeMode: ThemeMode): boolean {
  return themeVariantForMode(themeMode) === 'dark'
}

// Blend an overlay color into a base color with a given percentage (0-100)
// e.g. blend('#111111', '#FFFFFF', 6) creates 94% base + 6% overlay
function blend(base: string, overlay: string, overlayPercent: number) {
  const percent = Math.max(0, Math.min(100, overlayPercent))
  return `color-mix(in srgb, ${overlay} ${percent}%, ${base} ${100 - percent}%)`
}

// Alpha transparency helper
// e.g. alpha('#FFFFFF', 10) creates rgba with 10% opacity
function alpha(color: string, alphaPercent: number) {
  const percent = Math.max(0, Math.min(100, alphaPercent))
  return `color-mix(in srgb, ${color} ${percent}%, transparent ${100 - percent}%)`
}

// Sidebar frosted glass depends on native window translucency, which only
// macOS provides via vibrancy. Windows and Linux main windows are opaque, so
// a blurred translucent sidebar there only costs GPU with nothing real to
// blur behind it. Fail closed: unknown platform means no glass.
function sidebarGlassSupported(): boolean {
  return typeof document !== 'undefined' && document.documentElement.dataset.platform === 'darwin'
}

function resolveCompactUiVariables(
  theme: ThemeConfig['theme'],
  variant: ThemeVariant,
  baseThemeId?: ThemeBaseId
): Record<string, string> {
  const isLight = variant === 'light'
  const isCodex = baseThemeId === 'codex'
  const surface = theme.surface
  const surfaceSecondary = theme.surfaceSecondary
  const surfaceElevated = theme.surfaceElevated
  const ink = theme.ink
  const accent = theme.accent
  const secondaryAccent = theme.semanticColors.secondary

  const sidebar = isCodex ? surfaceSecondary : isLight ? surfaceElevated : '#242424'
  const card = surfaceSecondary
  const elevated = surfaceElevated
  const input = isCodex ? surfaceElevated : isLight ? surfaceElevated : '#1a1a1a'
  const hover =
    isCodex && !isLight
      ? '#2a2a2a'
      : isLight
        ? blend(surfaceElevated, '#000000', 5.5)
        : blend(surfaceElevated, '#FFFFFF', 8)
  const active = isCodex
    ? isLight
      ? '#e4e4e7'
      : '#333338'
    : isLight
      ? blend(surfaceElevated, accent, 12)
      : blend(surfaceElevated, accent, 18)
  const titlebar = isCodex ? surfaceElevated : isLight ? surface : '#272727'
  const tabbar = isCodex ? surfaceElevated : isLight ? surface : surfaceSecondary
  const managerHeadBg = isCodex ? surfaceSecondary : isLight ? surface : '#242424'

  const secondaryText = theme.semanticColors.textSecondary
  const softText = isLight ? blend(surface, secondaryText, 72) : blend(surface, secondaryText, 68)
  const disabledText = isLight ? blend(surface, secondaryText, 46) : blend(surface, secondaryText, 42)

  const border = alpha(ink, isLight ? 12 : 10)
  const strongBorder = alpha(ink, isLight ? 22 : 18)
  const subtleBorder = alpha(ink, isLight ? 8 : 6)

  const focus = secondaryAccent
  const accentHover = isLight ? blend(accent, '#000000', 12) : blend(accent, '#FFFFFF', 15)
  const secondaryHover = isLight ? blend(secondaryAccent, '#000000', 12) : blend(secondaryAccent, '#FFFFFF', 15)
  const accentText = isLight ? blend(accent, ink, 35) : blend(accent, '#FFFFFF', 70)

  const primaryAction = theme.semanticColors.primaryAction ?? accent
  const primaryActionHover = isLight ? blend(primaryAction, '#000000', 12) : blend(primaryAction, '#FFFFFF', 15)
  const dangerAction = theme.semanticColors.dangerAction ?? (isLight ? '#d32f2f' : '#c93b3b')
  const dangerActionHover = blend(dangerAction, '#000000', 12)

  const danger = theme.semanticColors.error
  const dangerHover = blend(danger, '#000000', 12)
  const success = theme.semanticColors.success
  const warning = theme.semanticColors.warning
  const info = theme.semanticColors.info
  const telnet =
    theme.semanticColors.telnet ??
    (theme.semanticColors as { sftp?: string }).sftp ??
    (isCodex ? (isLight ? '#0284c7' : '#38bdf8') : isLight ? '#0284c7' : '#38bdf8')
  const ftp = theme.semanticColors.ftp
  const total = theme.semanticColors.total ?? (isCodex && !isLight ? '#60a5fa' : isLight ? '#2563eb' : '#60a5fa')
  const networkRx =
    theme.semanticColors.networkRx ?? (isCodex ? (isLight ? '#0284c7' : '#38bdf8') : isLight ? '#3b82f6' : '#65a9ff')
  const networkTx =
    theme.semanticColors.networkTx ?? (isCodex ? (isLight ? '#ba2623' : '#f43f5e') : isLight ? '#ef4444' : '#ff7474')

  const secondarySurface = alpha(secondaryAccent, isLight ? 10 : 14)
  const totalSurface = alpha(total, isLight ? 10 : 14)
  const infoSurface = alpha(info, isLight ? 10 : 14)
  const successSurface = alpha(success, isLight ? 10 : 14)
  const warningSurface = alpha(warning, isLight ? 10 : 14)
  const dangerSurface = alpha(danger, isLight ? 10 : 14)

  const sidebarGlassActive = !theme.opaqueWindows && sidebarGlassSupported()
  const sidebarBackground = sidebarGlassActive ? alpha(sidebar, isLight ? 88 : 82) : sidebar

  const folderAccent = isCodex ? (isLight ? '#3b82f6' : '#fbbf24') : isLight ? '#3b82f6' : '#65a9ff'
  const kernelAccent = isCodex ? accent : isLight ? '#2563eb' : '#65a9ff'
  const copyLink = isCodex ? (isLight ? '#0284c7' : '#38bdf8') : isLight ? '#4f7cff' : '#65a9ff'
  const copyLinkHover = isCodex ? (isLight ? '#0369a1' : '#7dd3fc') : isLight ? '#2f5fef' : '#8bbfff'

  return {
    '--bg-main': surface,
    '--bg-primary': surface,
    '--bg-secondary': surfaceSecondary,
    '--bg-sidebar': sidebarBackground,
    '--bg-card': card,
    '--bg-elevated': elevated,
    '--bg-hover': hover,
    '--bg-active': active,
    '--titlebar-background': titlebar,
    '--tabbar-background': tabbar,
    '--manager-head-bg': managerHeadBg,
    '--input-bg': input,
    '--command-history-head-bg': surface,
    '--surface-panel': card,
    '--surface-raised': elevated,
    '--surface-secondary': surfaceSecondary,
    '--surface-elevated': surfaceElevated,
    '--surface-hover': alpha(ink, isLight ? 5 : 7),
    '--surface-chip': alpha(ink, isLight ? 8 : 14),
    '--surface-inset': alpha(ink, isLight ? 3 : 5),
    '--surface-inset-border': border,
    '--surface-table-head': alpha(ink, isLight ? 4 : 6),
    '--surface-nested': alpha(ink, isLight ? 3 : 5),
    '--overview-hero-highlight': alpha('#FFFFFF', 2),
    '--border-light': border,
    '--border-dark': strongBorder,
    '--border': border,
    '--border-subtle': subtleBorder,
    '--text-main': ink,
    '--text-primary': ink,
    '--text-secondary': secondaryText,
    '--text-muted': secondaryText,
    '--text-soft': softText,
    '--text-disabled': disabledText,
    '--muted-foreground': secondaryText,
    '--primary': accent,
    '--primary-hover': accentHover,
    '--secondary': secondaryText,
    '--secondary-accent': secondaryAccent,
    '--accent-secondary': secondaryAccent,
    '--theme-secondary': secondaryAccent,
    '--theme-surface-secondary': surfaceSecondary,
    '--theme-surface-elevated': surfaceElevated,
    '--theme-text-primary': ink,
    '--theme-text-secondary': secondaryText,
    '--theme-info': info,
    '--theme-semantic-total': total,
    '--theme-semantic-telnet': telnet,
    '--theme-semantic-sftp': telnet,
    '--theme-semantic-ftp': ftp,
    '--theme-semantic-network-rx': networkRx,
    '--theme-semantic-network-tx': networkTx,
    '--theme-warning': warning,
    '--theme-error': danger,
    '--theme-error-hover': dangerHover,
    '--theme-success': success,
    '--focus-outline': focus,
    '--border-focus': focus,
    '--accent-highlight': secondaryAccent,
    '--accent-text': accentText,
    '--sidebar-active-accent': ink,
    '--selection-bg': active,
    '--accent-tint-weak': alpha(focus, isLight ? 8 : 12),
    '--accent-tint': alpha(focus, isLight ? 14 : 20),
    '--accent-focus-ring': alpha(focus, isLight ? 24 : 28),
    '--input-focus-ring': alpha(focus, isLight ? 18 : 22),
    '--danger': danger,
    '--danger-text': danger,
    '--danger-surface': dangerSurface,
    '--danger-border': alpha(danger, isLight ? 20 : 30),
    '--success': success,
    '--success-text': success,
    '--success-surface': successSurface,
    '--success-border': alpha(success, isLight ? 20 : 30),
    '--warning': warning,
    '--info': info,
    '--info-text': secondaryHover,
    '--info-surface': infoSurface,
    '--info-border': alpha(info, isLight ? 20 : 30),
    '--folder-accent': folderAccent,
    '--kernel-accent': kernelAccent,
    '--copy-link': copyLink,
    '--copy-link-hover': copyLinkHover,
    '--mini-tab-active-bg': secondarySurface,
    '--mini-tab-active-text': secondaryHover,
    '--memory-warn': warning,
    '--network-tx': networkTx,
    '--network-rx': networkRx,
    '--button-primary-bg': isLight ? primaryAction : blend(primaryAction, '#000000', 25),
    '--button-primary-hover': isLight ? primaryActionHover : blend(primaryAction, '#000000', 12),
    '--button-primary-border': border,
    '--button-primary-text': '#FFFFFF',
    '--action-primary-bg': primaryAction,
    '--action-primary-hover': primaryActionHover,
    '--action-primary-text': '#FFFFFF',
    '--floating-drawer-expanded-bg': isLight ? alpha('#FFFFFF', 94) : alpha(surface, 92),
    '--floating-drawer-expanded-border': border,
    '--floating-drawer-shadow': isLight
      ? '0 10px 26px rgba(0, 0, 0, 0.08), 0 1px 2px rgba(0, 0, 0, 0.06)'
      : '0 4px 12px rgba(0, 0, 0, 0.25), 0 1px 3px rgba(0, 0, 0, 0.15)',
    '--floating-drawer-trigger-hover-bg': alpha(ink, isLight ? 6 : 10),
    '--modal-backdrop-bg': isLight ? 'rgba(0, 0, 0, 0.32)' : 'rgba(0, 0, 0, 0.62)',
    '--modal-card-shadow': isLight ? '0 20px 50px rgba(0, 0, 0, 0.12)' : '0 20px 60px rgba(0, 0, 0, 0.5)',
    '--control-shadow': isLight ? '0 1px 2px rgba(0, 0, 0, 0.05)' : '0 1px 2px rgba(0, 0, 0, 0.15)',
    '--control-focus-shadow': `0 0 0 2px ${alpha(accent, isLight ? 14 : 18)}, 0 1px 2px rgba(0, 0, 0, 0.1)`,
    '--control-inset-shadow': isLight ? 'inset 0 1px 2px rgba(0, 0, 0, 0.04)' : 'inset 0 1px 2px rgba(0, 0, 0, 0.15)',
    '--terminal-dock-shadow': isLight ? '0 12px 32px rgba(0, 0, 0, 0.1)' : '0 16px 36px rgba(0, 0, 0, 0.35)',
    '--terminal-dock-header-shadow': isLight ? '0 8px 20px rgba(0, 0, 0, 0.08)' : '0 10px 24px rgba(0, 0, 0, 0.25)',
    '--system-sidebar-toggle-shadow': isLight ? '0 4px 12px rgba(0, 0, 0, 0.06)' : '0 8px 18px rgba(15, 23, 42, 0.12)',
    '--system-sidebar-toggle-hover-shadow': isLight
      ? '0 6px 16px rgba(0, 0, 0, 0.08)'
      : '0 10px 22px rgba(15, 23, 42, 0.16)',
    '--popover-bg': elevated,
    '--popover-border': border,
    '--popover-shadow': isLight ? '0 12px 32px rgba(15, 23, 42, 0.12)' : '0 20px 40px rgba(0, 0, 0, 0.4)',
    '--brand-title-shadow': isLight ? 'none' : '0 2px 10px rgba(0, 0, 0, 0.3)',
    '--traffic-close': '#ff5f57',
    '--traffic-minimize': '#ffbd2e',
    '--traffic-maximize': '#27c93f',
    '--confirm-dialog-text': secondaryText,
    '--dialog-surface': card,
    '--dialog-border': border,
    '--dialog-shadow': isLight
      ? '0 22px 70px rgba(15, 23, 42, 0.16), 0 1px 0 rgba(255, 255, 255, 0.82) inset'
      : '0 22px 70px rgba(0, 0, 0, 0.45), 0 1px 0 rgba(255, 255, 255, 0.04) inset',
    '--dialog-title': ink,
    '--dialog-description': secondaryText,
    '--dialog-footer-border': border,
    '--dialog-focus-ring': `0 0 0 2px ${blend(card, 'transparent', 12)}, 0 0 0 4px ${alpha(secondaryAccent, isLight ? 24 : 28)}`,
    '--dialog-warning-text': danger,
    '--dialog-button-secondary-bg': 'transparent',
    '--dialog-button-secondary-border': strongBorder,
    '--dialog-button-secondary-text': ink,
    '--dialog-button-secondary-hover-bg': hover,
    '--dialog-button-secondary-hover-border': strongBorder,
    '--dialog-button-primary-bg': hover,
    '--dialog-button-primary-border': strongBorder,
    '--dialog-button-primary-text': ink,
    '--dialog-button-primary-hover-bg': active,
    '--dialog-button-primary-hover-border': strongBorder,
    '--dialog-button-danger-bg': dangerAction,
    '--dialog-button-danger-border': 'transparent',
    '--dialog-button-danger-text': '#ffffff',
    '--dialog-button-danger-hover-bg': dangerActionHover,
    '--dialog-button-danger-hover-border': 'transparent',
    '--dialog-button-danger-hover-text': '#ffffff',
    '--action-danger-bg': dangerAction,
    '--action-danger-hover': dangerActionHover,
    '--action-danger-text': '#ffffff',
    '--theme-action-primary': primaryAction,
    '--theme-action-primary-hover': primaryActionHover,
    '--theme-action-danger': dangerAction,
    '--theme-action-danger-hover': dangerActionHover,
    '--theme-sidebar-background': sidebarBackground,
    '--theme-sidebar-backdrop-filter': sidebarGlassActive ? 'blur(18px)' : 'none',
    '--window-control-surface': elevated,
    '--window-control-hover-surface': hover,
    '--window-control-foreground': secondaryText,
    '--window-control-hover-foreground': ink,
    '--window-control-danger-bg': '#c42b1c',
    '--window-control-danger-text': '#ffffff',
    '--system-sidebar-frame': border,
    '--system-sidebar-control-border': border,
    '--system-sidebar-divider': border,
    '--file-editor-button-bg': alpha(ink, isLight ? 4 : 6),
    '--file-editor-primary-shadow': `0 2px 10px ${alpha(accent, 35)}`,
    '--terminal-cmd-bg': alpha(ink, isLight ? 8 : 16),
    '--terminal-cmd-text': ink,
    '--terminal-right-frame-outer': sidebar,
    '--terminal-right-frame-accent': border,
    '--theme-terminal-dock-surface': surfaceElevated,
    '--terminal-dock-surface': elevated,
    '--terminal-dock-text-muted': secondaryText,
    '--terminal-find-action-divider': 'transparent',
    '--terminal-dock-border': border,
    '--type-total': total,
    '--type-total-surface': totalSurface,
    '--type-ssh': success,
    '--type-ssh-surface': successSurface,
    // Connection overview types have their own controls. In particular,
    // Telnet and FTP must not consume the secondary focus/outline color.
    '--type-telnet': telnet,
    '--type-telnet-surface': alpha(telnet, isLight ? 10 : 14),
    '--type-sftp': telnet,
    '--type-sftp-surface': alpha(telnet, isLight ? 10 : 14),
    '--type-ftp': ftp,
    '--type-ftp-surface': alpha(ftp, isLight ? 10 : 14),
    '--type-folder': warning,
    '--type-folder-surface': warningSurface,
    '--type-muted': softText,
    '--metric-app': warning,
    '--metric-cache': success,
    '--metric-kernel': accent,
    '--metric-status-green': success,
    '--metric-status-yellow': warning,
    '--metric-status-red': danger,
    '--meter-track': alpha(ink, isLight ? 12 : 18),
    '--monaco-editor-bg': theme.terminal.background,
    '--monaco-editor-foreground': theme.terminal.foreground,
    '--monaco-line-number': softText,
    '--monaco-line-number-active': secondaryText,
    '--monaco-cursor': accent,
    '--monaco-selection': active,
    '--monaco-inactive-selection': alpha(accent, isLight ? 12 : 18),
    '--monaco-line-highlight': alpha(ink, isLight ? 4 : 6),
    '--monaco-indent-guide': border,
    '--monaco-indent-guide-active': strongBorder
  }
}

function applyThemeOverrides(variables: Record<string, string>, overrides: Record<string, string> | undefined) {
  if (!overrides) return
  for (const [key, value] of Object.entries(overrides)) {
    if (/^--[a-z0-9-]+$/i.test(key)) {
      variables[key] = value
    }
  }
}

function isDefaultFileTermTheme(config: ThemeConfig, variant: ThemeVariant): boolean {
  const defaultConfig = createDefaultThemeConfig(variant)
  // The built-in FileTerm id is authoritative. Older releases persisted the
  // same id with a few legacy token values; comparing every color here would
  // incorrectly route those preferences through the custom/Codex skin. Keep
  // the built-in id on the historical CSS skin and only inspect values for a
  // configuration explicitly marked as custom.
  const isFileTermPreset = config.codeThemeId === defaultConfig.codeThemeId
  const isUnmodifiedFileTermCustom = config.codeThemeId === 'custom' && config.baseThemeId === 'fileterm'
  if (isFileTermPreset) {
    return !config.theme.overrides || Object.keys(config.theme.overrides).length === 0
  }
  if (!isUnmodifiedFileTermCustom) return false
  if (config.theme.overrides && Object.keys(config.theme.overrides).length > 0) return false
  const semanticColorKeys: Array<keyof ThemeConfig['theme']['semanticColors']> = [
    'diffAdded',
    'diffRemoved',
    'skill',
    'keyword',
    'total',
    'telnet',
    'ftp',
    'networkRx',
    'networkTx',
    'secondary',
    'textSecondary',
    'info',
    'warning',
    'error',
    'success',
    'primaryAction',
    'dangerAction'
  ]
  return (
    config.theme.accent.toUpperCase() === defaultConfig.theme.accent.toUpperCase() &&
    config.theme.surface.toUpperCase() === defaultConfig.theme.surface.toUpperCase() &&
    config.theme.surfaceSecondary.toUpperCase() === defaultConfig.theme.surfaceSecondary.toUpperCase() &&
    config.theme.surfaceElevated.toUpperCase() === defaultConfig.theme.surfaceElevated.toUpperCase() &&
    config.theme.ink.toUpperCase() === defaultConfig.theme.ink.toUpperCase() &&
    config.theme.contrast === defaultConfig.theme.contrast &&
    config.theme.opaqueWindows === defaultConfig.theme.opaqueWindows &&
    semanticColorKeys.every((key) => {
      const currentVal = config.theme.semanticColors[key] ?? defaultConfig.theme.semanticColors[key]
      const defaultVal = defaultConfig.theme.semanticColors[key]
      if (!currentVal || !defaultVal) return true
      return currentVal.toUpperCase() === defaultVal.toUpperCase()
    })
  )
}

function isBuiltInTheme(config: ThemeConfig) {
  return config.codeThemeId === 'fileterm' || config.codeThemeId === 'codex'
}

function baseThemeConfigFor(config: ThemeConfig, variant: ThemeVariant): ThemeConfig | null {
  const baseThemeId: ThemeBaseId | undefined = config.baseThemeId
  if (baseThemeId === 'codex') return createCodexThemeConfig(variant)
  if (baseThemeId === 'fileterm') return createDefaultThemeConfig(variant)
  if (config.codeThemeId === 'codex') return createCodexThemeConfig(variant)
  if (config.codeThemeId === 'fileterm') return createDefaultThemeConfig(variant)
  return null
}

const THEME_VARIANT_KEYS = [
  'accent',
  'contrast',
  'fonts',
  'ink',
  'opaqueWindows',
  'semanticColors',
  'surface',
  'surfaceSecondary',
  'surfaceElevated',
  'overrides',
  'terminal'
] as const

function cloneThemeValue<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function sameThemeValue(left: unknown, right: unknown) {
  return JSON.stringify(left) === JSON.stringify(right)
}

/**
 * Creates the other light/dark variant of a custom theme. Values that still
 * match the selected base preset follow that preset's other variant; values
 * explicitly changed by the user are carried across unchanged.
 */
export function deriveThemeVariant(config: ThemeConfig, targetVariant: ThemeVariant): ThemeConfig {
  const source = normalizeThemeConfig(config, config.variant)
  if (source.variant === targetVariant) return source

  const sourceBase = baseThemeConfigFor(source, source.variant) ?? createDefaultThemeConfig(source.variant)
  const targetBase = baseThemeConfigFor(source, targetVariant) ?? createDefaultThemeConfig(targetVariant)
  const target = cloneThemeValue(targetBase)
  target.codeThemeId = source.codeThemeId
  target.baseThemeId = source.baseThemeId ?? targetBase.baseThemeId
  target.variant = targetVariant

  for (const key of THEME_VARIANT_KEYS) {
    if (!sameThemeValue(source.theme[key], sourceBase.theme[key])) {
      Object.assign(target.theme, { [key]: cloneThemeValue(source.theme[key]) })
    }
  }

  return normalizeThemeConfig(target, targetVariant)
}

export function getSavedThemeConfig(savedTheme: SavedTheme, variant: ThemeVariant): ThemeConfig {
  const directVariant = savedTheme.variants?.[variant]
  if (directVariant) return normalizeThemeConfig({ ...directVariant, variant }, variant)

  const source = normalizeThemeConfig(savedTheme.config, savedTheme.config.variant)
  return deriveThemeVariant(source, variant)
}

export function normalizeSavedTheme(savedTheme: SavedTheme): SavedTheme {
  const current = normalizeThemeConfig(savedTheme.config, savedTheme.config.variant)
  const variants = {
    dark: savedTheme.variants?.dark
      ? normalizeThemeConfig({ ...savedTheme.variants.dark, variant: 'dark' }, 'dark')
      : deriveThemeVariant(current, 'dark'),
    light: savedTheme.variants?.light
      ? normalizeThemeConfig({ ...savedTheme.variants.light, variant: 'light' }, 'light')
      : deriveThemeVariant(current, 'light')
  }
  return { ...savedTheme, config: current, variants }
}

function buildThemeVariables(
  themeMode: ThemeMode,
  config: ThemeConfig,
  forceCompactSkin = false
): { normalized: ThemeConfig; variables: Record<string, string>; isDefaultTheme: boolean } {
  const variant = themeVariantForMode(themeMode)
  const normalized = normalizeThemeConfig({ ...config, variant }, variant)
  const theme = normalized.theme
  const isDefaultTheme = isDefaultFileTermTheme(normalized, variant)
  const sidebarGlassActive = !theme.opaqueWindows && sidebarGlassSupported()
  const variables: Record<string, string> = {
    '--theme-accent': theme.accent,
    '--theme-accent-hover': blend(theme.accent, theme.ink, 8),
    '--theme-contrast': `${theme.contrast}%`,
    '--theme-contrast-soft': `${Math.round(theme.contrast * 0.25)}%`,
    '--theme-contrast-medium': `${Math.round(theme.contrast * 0.5)}%`,
    '--theme-contrast-strong': `${Math.round(theme.contrast)}%`,
    '--theme-surface': theme.surface,
    '--theme-surface-secondary': theme.surfaceSecondary,
    '--theme-surface-elevated': theme.surfaceElevated,
    '--theme-ink': theme.ink,
    '--theme-secondary': theme.semanticColors.secondary,
    '--theme-text-primary': theme.ink,
    '--theme-text-secondary': theme.semanticColors.textSecondary,
    '--theme-info': theme.semanticColors.info,
    '--theme-warning': theme.semanticColors.warning,
    '--theme-error': theme.semanticColors.error,
    '--theme-success': theme.semanticColors.success,
    '--theme-action-primary': theme.semanticColors.primaryAction ?? theme.accent,
    '--theme-action-danger': theme.semanticColors.dangerAction ?? (variant === 'light' ? '#d32f2f' : '#c93b3b'),
    '--theme-sidebar-backdrop-filter': sidebarGlassActive ? 'blur(18px)' : 'none',
    '--theme-font-ui': theme.fonts.ui ?? 'var(--font-ui)',
    '--theme-font-code': theme.fonts.code ?? 'var(--font-mono)',
    '--theme-semantic-diff-added': theme.semanticColors.diffAdded,
    '--theme-semantic-diff-removed': theme.semanticColors.diffRemoved,
    '--theme-semantic-skill': theme.semanticColors.skill,
    '--theme-semantic-keyword': theme.semanticColors.keyword,
    '--theme-semantic-total': theme.semanticColors.total,
    '--theme-semantic-telnet': theme.semanticColors.telnet,
    '--theme-semantic-sftp': theme.semanticColors.telnet,
    '--theme-semantic-ftp': theme.semanticColors.ftp,
    '--theme-semantic-network-rx': theme.semanticColors.networkRx,
    '--theme-semantic-network-tx': theme.semanticColors.networkTx,
    '--terminal-background': theme.terminal.background,
    '--terminal-foreground': theme.terminal.foreground,
    '--terminal-cursor': theme.terminal.cursor,
    '--terminal-cursor-accent': theme.terminal.cursorAccent,
    '--terminal-selection-background': theme.terminal.selectionBackground,
    '--terminal-selection-foreground': theme.terminal.selectionForeground,
    '--terminal-search-match-background': theme.terminal.search.matchBackground,
    '--terminal-search-match-ruler': theme.terminal.search.matchRuler,
    '--terminal-search-active-background': theme.terminal.search.activeMatchBackground,
    '--terminal-search-active-text': theme.terminal.search.activeMatchText,
    '--terminal-search-active-border': theme.terminal.search.activeMatchBorder,
    '--terminal-search-active-ruler': theme.terminal.search.activeMatchRuler,
    // Compatibility aliases used by the existing terminal and editor skins.
    '--terminal-bg': theme.terminal.background,
    '--terminal-text': theme.terminal.foreground,
    '--terminal-selection-bg': theme.terminal.selectionBackground,
    '--terminal-search-match-bg': theme.terminal.search.matchBackground,
    '--terminal-search-active-bg': theme.terminal.search.activeMatchBackground
  }

  // Built-in Codex has the same compact token contract as custom themes, but
  // its base config must still win over the legacy default-dark stylesheet.
  if (forceCompactSkin || !isDefaultTheme || normalized.baseThemeId === 'codex') {
    Object.assign(variables, resolveCompactUiVariables(theme, variant, normalized.baseThemeId))
  }
  applyThemeOverrides(variables, theme.overrides)

  for (const [colorName, variableName] of ANSI_VARIABLE_NAMES) {
    variables[variableName] = theme.terminal.ansi[colorName]
  }
  if (theme.fonts.ui) {
    variables['--font-ui'] = theme.fonts.ui
  }
  if (theme.fonts.code) {
    variables['--font-mono'] = theme.fonts.code
  }

  return { normalized, variables, isDefaultTheme }
}

function applyRootVariables(root: HTMLElement, themeMode: ThemeMode, config: ThemeConfig) {
  for (const name of appliedThemeVariableNames) {
    root.style.removeProperty(name)
  }
  appliedThemeVariableNames.clear()

  const variant = themeVariantForMode(themeMode)
  const normalizedInput = normalizeThemeConfig({ ...config, variant }, variant)
  const current = buildThemeVariables(themeMode, normalizedInput, !isBuiltInTheme(normalizedInput))
  const customTheme = !isBuiltInTheme(current.normalized)
  const baseConfig = customTheme ? baseThemeConfigFor(current.normalized, variant) : null
  let variables = current.variables

  if (baseConfig) {
    const base = buildThemeVariables(themeMode, baseConfig)
    const baseComparison = buildThemeVariables(themeMode, baseConfig, true).variables
    variables = { ...base.variables }
    for (const [name, value] of Object.entries(current.variables)) {
      if (baseComparison[name] !== value) {
        variables[name] = value
      }
    }
  }

  const actualThemeMode =
    current.normalized.baseThemeId === 'codex'
      ? variant === 'light'
        ? 'codex-light'
        : 'codex-dark'
      : variant === 'light'
        ? 'fileterm-light'
        : 'fileterm-dark'

  root.dataset.theme = actualThemeMode
  root.dataset.themeBase = current.normalized.baseThemeId ?? 'fileterm'
  root.style.colorScheme = variant
  // Keep the untouched FileTerm preset on its historical skin. Every
  // non-default theme, including a custom theme based on FileTerm, must use
  // the same token-to-region bridge as Codex so the settings fields control
  // the same UI areas in both themes.
  root.dataset.themeCustom = current.isDefaultTheme ? 'false' : 'true'

  for (const [name, value] of Object.entries(variables)) {
    root.style.setProperty(name, value)
    appliedThemeVariableNames.add(name)
  }
}

export function applyThemeVariables(themeMode: ThemeMode, config?: ThemeConfig) {
  const root = document.documentElement
  applyRootVariables(root, themeMode, config ?? createDefaultThemeConfig(themeVariantForMode(themeMode)))
}

export function clearThemeVariables() {
  const root = document.documentElement
  for (const name of appliedThemeVariableNames) {
    root.style.removeProperty(name)
  }
  appliedThemeVariableNames.clear()
  delete root.dataset.theme
  delete root.dataset.themeCustom
  delete root.dataset.themeBase
}
