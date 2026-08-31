import { useEffect } from 'react'
import {
  createDefaultThemeConfig,
  normalizeThemeConfig,
  type ImportedFont,
  type TerminalAnsiColorName,
  type ThemeConfig
} from '@fileterm/core'
import { deriveThemeVariant, getSavedThemeConfig, normalizeSavedTheme } from '../../../../app/theme-config'
import { registerImportedFont, unregisterImportedFont } from '../../../../app/imported-fonts'
import { t } from '../../../../i18n'
import {
  THEME_CONFIG_EXPORT_PREFIX,
  THEME_CONFIG_IMPORT_PREFIXES,
  THEME_PRESETS,
  clipboardUnavailableError,
  createCustomThemeId,
  findMatchingThemePreset,
  findSavedThemeForConfig,
  sameThemeConfig,
  themeBaseIdForConfig,
  type ThemePresetVariant
} from '../constants'
import type { SettingsModalControllerOptions } from './types'
import type { SettingsModalState } from './state'

export function useThemeSettingsController({
  state,
  options
}: {
  state: SettingsModalState
  options: Pick<
    SettingsModalControllerOptions,
    'theme' | 'themeConfig' | 'customThemes' | 'onSetTheme' | 'onSetThemeConfig' | 'onSetCustomThemes'
  >
}) {
  const { theme, themeConfig, customThemes, onSetTheme, onSetThemeConfig, onSetCustomThemes } = options
  const {
    desktopApi,
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
    setShowDeleteThemeConfirm
  } = state

  const themeVariant = theme === 'default-light' ? 'light' : 'dark'
  const normalizedThemeConfig = normalizeThemeConfig(themeConfig, themeVariant)

  const setThemeConfigValue = (nextValue: ThemeConfig) => {
    onSetThemeConfig(
      normalizeThemeConfig(
        {
          ...nextValue,
          codeThemeId: 'custom',
          baseThemeId: themeBaseIdForConfig(nextValue)
        },
        themeVariant
      )
    )
    setThemeConfigMessage(null)
  }

  const updateThemeBody = (patch: Partial<ThemeConfig['theme']>) => {
    setThemeConfigValue({
      ...normalizedThemeConfig,
      theme: {
        ...normalizedThemeConfig.theme,
        ...patch
      }
    })
  }

  const updateThemeSemanticColors = (patch: Partial<ThemeConfig['theme']['semanticColors']>) => {
    updateThemeBody({
      semanticColors: {
        ...normalizedThemeConfig.theme.semanticColors,
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

  const importFontFor = async (kind: 'ui' | 'code') => {
    if (!desktopApi || fontImportKind) return

    setFontImportKind(kind)
    setFontImportError(null)
    try {
      const font = await desktopApi.importFont()
      if (!font) return

      const dataUrl = await desktopApi.getImportedFontData(font.id)
      if (dataUrl) registerImportedFont(font, dataUrl)
      setImportedFonts((current) => [font, ...current.filter((item) => item.id !== font.id)])
      updateThemeFonts({ [kind]: font.family })
    } catch (cause: unknown) {
      console.error('[FileTerm] 导入字体', cause)
      setFontImportError(t.themeFontImportFailed)
    } finally {
      setFontImportKind(null)
    }
  }

  const handleDeleteFont = async (font: ImportedFont) => {
    if (!desktopApi) return
    try {
      const success = await desktopApi.deleteImportedFont(font.id)
      if (!success) {
        setFontImportError(t.themeFontDeleteFailed)
        return
      }
      unregisterImportedFont(font.id)
      setImportedFonts((current) => current.filter((item) => item.id !== font.id))

      const patch: Partial<ThemeConfig['theme']['fonts']> = {}
      if (themeConfig.theme.fonts.ui === font.family) {
        patch.ui = null
      }
      if (themeConfig.theme.fonts.code === font.family) {
        patch.code = null
      }
      if (Object.keys(patch).length > 0) {
        updateThemeFonts(patch)
      }
      setFontToDelete(null)
      setFontImportError(null)
    } catch (cause: unknown) {
      console.error('[FileTerm] 删除字体', cause)
      setFontImportError(t.themeFontDeleteFailed)
    }
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
    if (presetId === 'custom') {
      const nextThemeConfig = normalizeThemeConfig(
        {
          ...createDefaultThemeConfig(variant),
          codeThemeId: 'custom',
          baseThemeId: 'fileterm'
        },
        variant
      )
      onSetThemeConfig(nextThemeConfig)
      onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
      setEditingCustomThemeId(null)
      setCustomThemeName('')
      setThemeConfigMessage({ text: t.themePresetApplied, kind: 'success' })
      return
    }

    if (presetId.startsWith('saved:')) {
      const savedId = presetId.slice('saved:'.length)
      const savedTheme = customThemes.find((candidate) => candidate.id === savedId)
      if (!savedTheme) return
      const nextThemeConfig = getSavedThemeConfig(savedTheme, variant)
      onSetThemeConfig(nextThemeConfig)
      onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
      setEditingCustomThemeId(savedTheme.id)
      setCustomThemeName(savedTheme.name)
      setThemeConfigMessage({ text: t.themePresetApplied, kind: 'success' })
      return
    }

    const preset = THEME_PRESETS.find((candidate) => candidate.id === presetId)
    if (!preset) return
    const nextThemeConfig = normalizeThemeConfig(preset.config[variant], variant)
    onSetThemeConfig(nextThemeConfig)
    onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
    setEditingCustomThemeId(null)
    setCustomThemeName('')
    setThemeConfigMessage({ text: t.themePresetApplied, kind: 'success' })
  }

  const saveCustomTheme = () => {
    const name = customThemeName.trim()
    if (!name) {
      setThemeConfigMessage({ text: t.themeNameRequired, kind: 'warning' })
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
    const normalizedExistingTheme = existingTheme ? normalizeSavedTheme(existingTheme) : null
    const variants = {
      dark: normalizedExistingTheme?.variants?.dark ?? deriveThemeVariant(nextThemeConfig, 'dark'),
      light: normalizedExistingTheme?.variants?.light ?? deriveThemeVariant(nextThemeConfig, 'light')
    }
    variants[themeVariant] = nextThemeConfig
    const nextCustomThemes = [
      ...customThemes.filter((candidate) => candidate.id !== id),
      { id, name, config: nextThemeConfig, variants }
    ]

    onSetCustomThemes(nextCustomThemes)
    onSetThemeConfig(nextThemeConfig)
    onSetTheme(nextThemeConfig.variant === 'light' ? 'default-light' : 'default-dark')
    setEditingCustomThemeId(id)
    setCustomThemeName(name)
    setThemeConfigMessage({ text: existingTheme ? t.themeUpdated : t.themeSaved, kind: 'success' })
  }

  const deleteCustomTheme = () => {
    if (!selectedSavedTheme) return
    const idToDelete = selectedSavedTheme.id
    const nextCustomThemes = customThemes.filter((candidate) => candidate.id !== idToDelete)
    onSetCustomThemes(nextCustomThemes)
    setEditingCustomThemeId(null)
    setCustomThemeName('')
    applyThemePreset('fileterm')
    setThemeConfigMessage({ text: t.themeDeleted, kind: 'success' })
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

    const savedTheme =
      (editingCustomThemeId ? customThemes.find((candidate) => candidate.id === editingCustomThemeId) : undefined) ??
      findSavedThemeForConfig(customThemes, themeConfig)
    if (savedTheme) {
      const currentSavedVariant = getSavedThemeConfig(savedTheme, themeConfig.variant)
      const nextThemeConfig = sameThemeConfig(currentSavedVariant, themeConfig)
        ? getSavedThemeConfig(savedTheme, nextVariant)
        : deriveThemeVariant(themeConfig, nextVariant)
      onSetTheme(nextVariant === 'light' ? 'default-light' : 'default-dark')
      onSetThemeConfig(nextThemeConfig)
      return
    }

    const nextThemeConfig = deriveThemeVariant(themeConfig, nextVariant)
    onSetTheme(nextVariant === 'light' ? 'default-light' : 'default-dark')
    onSetThemeConfig(nextThemeConfig)
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
      setThemeConfigMessage({ text: t.themeImported, kind: 'success' })
    } catch {
      setThemeConfigMessage({ text: t.themeImportFailed, kind: 'error' })
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
      setThemeConfigMessage({ text: t.themeCopied, kind: 'success' })
    } catch {
      setThemeConfigMessage({ text: t.themeCopyFailed, kind: 'error' })
    } finally {
      endThemeConfigOperation()
    }
  }

  const selectedThemePreset = findMatchingThemePreset(themeConfig)
  const matchingSavedTheme = findSavedThemeForConfig(customThemes, themeConfig)
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

  return {
    importedFonts,
    fontImportKind,
    importFontFor,
    fontToDelete,
    setFontToDelete,
    handleDeleteFont,
    fontImportError,
    themeConfig,
    themeVariant,
    normalizedThemeConfig,
    onSetTheme,
    customThemes,
    themeConfigOperation,
    importThemeConfig,
    copyThemeConfig,
    themeConfigMessage,
    themePresetValue,
    themePresetLabel,
    themePresetCode,
    applyThemePreset,
    switchThemeVariant,
    customThemeName,
    setCustomThemeName,
    saveCustomTheme,
    selectedSavedTheme,
    editingSavedTheme,
    showDeleteThemeConfirm,
    setShowDeleteThemeConfirm,
    deleteCustomTheme,
    updateThemeBody,
    updateThemeSemanticColors,
    updateThemeFonts,
    updateTerminalTheme,
    updateTerminalAnsiColor,
    updateTerminalSearchColors
  }
}
