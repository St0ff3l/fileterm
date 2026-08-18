import {
  createCodexThemeConfig,
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type TerminalAnsiColorName,
  type ThemeBaseId,
  type ThemeConfig,
  type ThemeVariant
} from '@fileterm/core'

export type ThemeMode = 'default-dark' | 'default-light'

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

function themeVariantForMode(themeMode: ThemeMode): ThemeVariant {
  return themeMode === 'default-light' ? 'light' : 'dark'
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

  const sidebar = surfaceSecondary
  const card = surfaceSecondary
  const elevated = surfaceElevated
  const input = surfaceElevated
  // Codex uses the same quiet neutral hover/active treatment as its native
  // sidebar. Keep the primary blue for actions and status semantics instead
  // of tinting every navigation interaction blue.
  const hover =
    isCodex && !isLight
      ? '#3a3a3a'
      : isLight
        ? blend(surfaceElevated, '#000000', 5.5)
        : blend(surfaceElevated, '#FFFFFF', 8)
  const active = isCodex
    ? isLight
      ? '#e4e4e7'
      : '#3a3d42'
    : isLight
      ? blend(surfaceElevated, accent, 12)
      : blend(surfaceElevated, accent, 18)
  const titlebar = surfaceElevated
  const managerHeadBg = surfaceSecondary

  const secondaryText = theme.semanticColors.textSecondary
  const softText = isLight ? blend(surface, secondaryText, 72) : blend(surface, secondaryText, 68)
  const disabledText = isLight ? blend(surface, secondaryText, 46) : blend(surface, secondaryText, 42)

  const border = alpha(ink, isLight ? 12 : 10)
  const strongBorder = alpha(ink, isLight ? 22 : 18)
  const subtleBorder = alpha(ink, isLight ? 8 : 6)

  // The secondary semantic color is the focus/outline accent in the built-in
  // dark skins. Keep Codex aligned with the default dark theme instead of
  // silently switching its outlines back to the primary blue.
  const focus = secondaryAccent
  const accentHover = isLight ? blend(accent, '#000000', 12) : blend(accent, '#FFFFFF', 15)
  const secondaryHover = isLight ? blend(secondaryAccent, '#000000', 12) : blend(secondaryAccent, '#FFFFFF', 15)
  const accentText = isLight ? blend(accent, ink, 35) : blend(accent, '#FFFFFF', 70)

  const danger = theme.semanticColors.error
  const success = theme.semanticColors.success
  const warning = theme.semanticColors.warning
  const info = theme.semanticColors.info
  const sftp = theme.semanticColors.sftp
  const ftp = theme.semanticColors.ftp
  const total = isCodex && !isLight ? '#60a5fa' : accent

  const secondarySurface = alpha(secondaryAccent, isLight ? 10 : 14)
  const totalSurface = alpha(total, isLight ? 10 : 14)
  const infoSurface = alpha(info, isLight ? 10 : 14)
  const successSurface = alpha(success, isLight ? 10 : 14)
  const warningSurface = alpha(warning, isLight ? 10 : 14)
  const dangerSurface = alpha(danger, isLight ? 10 : 14)

  const sidebarBackground = theme.opaqueWindows ? sidebar : alpha(sidebar, isLight ? 88 : 82)

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
    '--tabbar-background': surfaceElevated,
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
    '--theme-semantic-sftp': sftp,
    '--theme-semantic-ftp': ftp,
    '--theme-warning': warning,
    '--theme-error': danger,
    '--theme-success': success,
    '--focus-outline': focus,
    '--accent-highlight': secondaryAccent,
    '--accent-text': accentText,
    '--sidebar-active-accent': ink,
    '--selection-bg': active,
    '--accent-tint-weak': alpha(accent, isLight ? 8 : 10),
    '--accent-tint': alpha(accent, isLight ? 14 : 16),
    '--accent-focus-ring': alpha(accent, isLight ? 24 : 28),
    '--input-focus-ring': alpha(accent, isLight ? 18 : 22),
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
    '--folder-accent': warning,
    '--kernel-accent': accent,
    '--copy-link': accent,
    '--copy-link-hover': accentHover,
    '--mini-tab-active-bg': secondarySurface,
    '--mini-tab-active-text': secondaryHover,
    '--memory-warn': warning,
    '--network-tx': danger,
    '--network-rx': info,
    '--button-primary-bg': isLight ? accent : blend(accent, '#000000', 25),
    '--button-primary-hover': isLight ? accentHover : blend(accent, '#000000', 12),
    '--button-primary-border': border,
    '--button-primary-text': '#FFFFFF',
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
    '--dialog-focus-ring': `0 0 0 2px ${blend(card, 'transparent', 12)}, 0 0 0 4px ${alpha(accent, isLight ? 24 : 28)}`,
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
    '--dialog-button-danger-bg': danger,
    '--dialog-button-danger-border': 'transparent',
    '--dialog-button-danger-text': '#ffffff',
    '--dialog-button-danger-hover-bg': accentHover,
    '--dialog-button-danger-hover-border': 'transparent',
    '--dialog-button-danger-hover-text': '#ffffff',
    '--theme-sidebar-background': sidebarBackground,
    '--theme-sidebar-backdrop-filter': theme.opaqueWindows ? 'none' : 'blur(18px)',
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
    // SFTP and FTP must not consume the secondary focus/outline color.
    '--type-sftp': sftp,
    '--type-sftp-surface': alpha(sftp, isLight ? 10 : 14),
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
    '--monaco-indent-guide-active': strongBorder,
    '--theme-window-opacity': theme.opaqueWindows ? '1' : '0.82'
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
  if (config.codeThemeId !== defaultConfig.codeThemeId) return false
  if (config.theme.overrides && Object.keys(config.theme.overrides).length > 0) return false
  const semanticColorKeys: Array<keyof ThemeConfig['theme']['semanticColors']> = [
    'diffAdded',
    'diffRemoved',
    'skill',
    'keyword',
    'sftp',
    'ftp',
    'secondary',
    'textSecondary',
    'info',
    'warning',
    'error',
    'success'
  ]
  return (
    config.theme.accent.toUpperCase() === defaultConfig.theme.accent.toUpperCase() &&
    config.theme.surface.toUpperCase() === defaultConfig.theme.surface.toUpperCase() &&
    config.theme.surfaceSecondary.toUpperCase() === defaultConfig.theme.surfaceSecondary.toUpperCase() &&
    config.theme.surfaceElevated.toUpperCase() === defaultConfig.theme.surfaceElevated.toUpperCase() &&
    config.theme.ink.toUpperCase() === defaultConfig.theme.ink.toUpperCase() &&
    config.theme.contrast === defaultConfig.theme.contrast &&
    config.theme.opaqueWindows === defaultConfig.theme.opaqueWindows &&
    semanticColorKeys.every(
      (key) => config.theme.semanticColors[key].toUpperCase() === defaultConfig.theme.semanticColors[key].toUpperCase()
    )
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

function buildThemeVariables(
  themeMode: ThemeMode,
  config: ThemeConfig,
  forceCompactSkin = false
): { normalized: ThemeConfig; variables: Record<string, string>; isDefaultTheme: boolean } {
  const variant = themeVariantForMode(themeMode)
  const normalized = normalizeThemeConfig({ ...config, variant }, variant)
  const theme = normalized.theme
  const isDefaultTheme = isDefaultFileTermTheme(normalized, variant)
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
    '--theme-window-opacity': theme.opaqueWindows ? '1' : '0.82',
    '--theme-sidebar-backdrop-filter': theme.opaqueWindows ? 'none' : 'blur(18px)',
    '--theme-font-ui': theme.fonts.ui ?? 'var(--font-ui)',
    '--theme-font-code': theme.fonts.code ?? 'var(--font-mono)',
    '--theme-semantic-diff-added': theme.semanticColors.diffAdded,
    '--theme-semantic-diff-removed': theme.semanticColors.diffRemoved,
    '--theme-semantic-skill': theme.semanticColors.skill,
    '--theme-semantic-keyword': theme.semanticColors.keyword,
    '--theme-semantic-sftp': theme.semanticColors.sftp,
    '--theme-semantic-ftp': theme.semanticColors.ftp,
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

  root.dataset.theme = themeMode
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
  delete root.dataset.themeCustom
  delete root.dataset.themeBase
}
