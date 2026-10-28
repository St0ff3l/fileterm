import {
  createCodexThemeConfig,
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type SavedTheme,
  type ThemeConfig,
  type ThemeVariant
} from '@fileterm/core'

function baseThemeConfigFor(config: ThemeConfig, variant: ThemeVariant): ThemeConfig {
  return config.baseThemeId === 'codex' || config.codeThemeId === 'codex'
    ? createCodexThemeConfig(variant)
    : createDefaultThemeConfig(variant)
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
  'surfaceSidebar',
  'surfaceInput',
  'surfaceContextMenu',
  'overrides',
  'terminal'
] as const

function cloneThemeValue<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T)
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

  const sourceBase = normalizeThemeConfig(baseThemeConfigFor(source, source.variant), source.variant)
  const targetBase = normalizeThemeConfig(baseThemeConfigFor(source, targetVariant), targetVariant)
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
