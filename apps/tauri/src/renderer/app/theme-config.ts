import {
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type TerminalAnsiColorName,
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

function resolveCompactUiVariables(theme: ThemeConfig['theme'], variant: ThemeVariant): Record<string, string> {
  const isLight = variant === 'light'
  const surface = theme.surface
  const ink = theme.ink
  const accent = theme.accent

  const sidebar = isLight ? blend(surface, '#000000', 3) : blend(surface, '#FFFFFF', 5)
  const card = isLight ? '#FFFFFF' : blend(surface, '#FFFFFF', 4)
  const elevated = isLight ? '#FFFFFF' : blend(surface, '#FFFFFF', 8)
  const input = isLight ? '#FFFFFF' : blend(surface, '#000000', 18)
  const hover = isLight ? blend(surface, '#000000', 5.5) : blend(surface, '#FFFFFF', 8)
  const active = isLight ? blend(surface, accent, 12) : blend(surface, accent, 18)
  const titlebar = isLight ? surface : blend(surface, '#000000', 8)
  const managerHeadBg = isLight ? blend(surface, '#000000', 2) : sidebar

  const secondaryText = isLight ? blend(surface, ink, 68) : blend(surface, ink, 64)
  const softText = isLight ? blend(surface, ink, 48) : blend(surface, ink, 44)
  const disabledText = isLight ? blend(surface, ink, 28) : blend(surface, ink, 26)

  const border = alpha(ink, isLight ? 12 : 10)
  const strongBorder = alpha(ink, isLight ? 22 : 18)
  const subtleBorder = alpha(ink, isLight ? 8 : 6)

  const focus = accent
  const accentHover = isLight ? blend(accent, '#000000', 12) : blend(accent, '#FFFFFF', 15)
  const accentText = isLight ? blend(accent, ink, 35) : blend(accent, '#FFFFFF', 70)

  const danger = isLight ? '#DC2626' : '#FF5F57'
  const success = isLight ? '#16A34A' : '#39D98A'
  const warning = isLight ? '#D97706' : '#FFCC00'
  const info = accent

  const secondarySurface = alpha(accent, isLight ? 10 : 14)
  const successSurface = alpha(success, isLight ? 10 : 14)
  const warningSurface = alpha(warning, isLight ? 10 : 14)
  const dangerSurface = alpha(danger, isLight ? 10 : 14)

  const sidebarBackground = theme.opaqueWindows ? sidebar : alpha(sidebar, isLight ? 88 : 82)

  return {
    '--bg-main': surface,
    '--bg-primary': surface,
    '--bg-secondary': sidebar,
    '--bg-sidebar': sidebarBackground,
    '--bg-card': card,
    '--bg-elevated': elevated,
    '--bg-hover': hover,
    '--bg-active': active,
    '--titlebar-background': titlebar,
    '--manager-head-bg': managerHeadBg,
    '--input-bg': input,
    '--command-history-head-bg': surface,
    '--surface-panel': card,
    '--surface-raised': elevated,
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
    '--focus-outline': focus,
    '--accent-highlight': accent,
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
    '--info-text': accentHover,
    '--info-surface': secondarySurface,
    '--info-border': alpha(info, isLight ? 20 : 30),
    '--folder-accent': warning,
    '--kernel-accent': accent,
    '--copy-link': accent,
    '--copy-link-hover': accentHover,
    '--mini-tab-active-bg': secondarySurface,
    '--mini-tab-active-text': accentHover,
    '--memory-warn': warning,
    '--network-tx': danger,
    '--network-rx': accent,
    '--button-primary-bg': accent,
    '--button-primary-hover': accentHover,
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
    '--terminal-dock-surface': elevated,
    '--terminal-dock-text-muted': secondaryText,
    '--terminal-find-action-divider': 'transparent',
    '--terminal-dock-border': border,
    '--type-total': accent,
    '--type-total-surface': secondarySurface,
    '--type-ssh': success,
    '--type-ssh-surface': successSurface,
    '--type-sftp': info,
    '--type-sftp-surface': secondarySurface,
    '--type-ftp': theme.semanticColors.skill,
    '--type-ftp-surface': alpha(theme.semanticColors.skill, isLight ? 10 : 14),
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
  return (
    config.theme.accent.toUpperCase() === defaultConfig.theme.accent.toUpperCase() &&
    config.theme.surface.toUpperCase() === defaultConfig.theme.surface.toUpperCase() &&
    config.theme.ink.toUpperCase() === defaultConfig.theme.ink.toUpperCase() &&
    config.theme.contrast === defaultConfig.theme.contrast &&
    config.theme.opaqueWindows === defaultConfig.theme.opaqueWindows
  )
}

function applyRootVariables(root: HTMLElement, themeMode: ThemeMode, config: ThemeConfig) {
  for (const name of appliedThemeVariableNames) {
    root.style.removeProperty(name)
  }
  appliedThemeVariableNames.clear()

  const variant = themeVariantForMode(themeMode)
  const normalized = normalizeThemeConfig({ ...config, variant }, variant)
  const theme = normalized.theme
  const isDefaultTheme = isDefaultFileTermTheme(normalized, variant)
  root.dataset.theme = themeMode
  root.style.colorScheme = variant
  const variables: Record<string, string> = {
    '--theme-accent': theme.accent,
    '--theme-accent-hover': blend(theme.accent, theme.ink, 8),
    '--theme-contrast': `${theme.contrast}%`,
    '--theme-contrast-soft': `${Math.round(theme.contrast * 0.25)}%`,
    '--theme-contrast-medium': `${Math.round(theme.contrast * 0.5)}%`,
    '--theme-contrast-strong': `${Math.round(theme.contrast)}%`,
    '--theme-surface': theme.surface,
    '--theme-ink': theme.ink,
    '--theme-window-opacity': theme.opaqueWindows ? '1' : '0.82',
    '--theme-sidebar-backdrop-filter': theme.opaqueWindows ? 'none' : 'blur(18px)',
    '--theme-font-ui': theme.fonts.ui ?? 'var(--font-ui)',
    '--theme-font-code': theme.fonts.code ?? 'var(--font-mono)',
    '--theme-semantic-diff-added': theme.semanticColors.diffAdded,
    '--theme-semantic-diff-removed': theme.semanticColors.diffRemoved,
    '--theme-semantic-skill': theme.semanticColors.skill,
    '--theme-semantic-keyword': theme.semanticColors.keyword,
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

  if (!isDefaultTheme) {
    Object.assign(variables, resolveCompactUiVariables(theme, variant))
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
}
