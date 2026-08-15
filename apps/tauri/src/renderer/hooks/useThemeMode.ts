import { useEffect } from 'react'
import type { ThemeConfig } from '@fileterm/core'
import { applyThemeVariables, clearThemeVariables, type ThemeMode } from '../app/theme-config'

export type { ThemeMode } from '../app/theme-config'

export function useThemeMode(themeName: ThemeMode = 'default-dark', themeConfig?: ThemeConfig) {
  useEffect(() => {
    applyThemeVariables(themeName, themeConfig)
    return () => {
      if (document.documentElement.dataset.theme === themeName) {
        clearThemeVariables()
        delete document.documentElement.dataset.theme
      }
      if (document.documentElement.style.colorScheme === (themeName === 'default-light' ? 'light' : 'dark')) {
        document.documentElement.style.removeProperty('color-scheme')
      }
    }
  }, [themeConfig, themeName])
}
