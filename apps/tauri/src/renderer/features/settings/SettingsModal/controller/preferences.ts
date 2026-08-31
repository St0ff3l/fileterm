import {
  DEFAULT_LOCAL_TERMINAL_SHELLS,
  DEFAULT_SSH_CONNECTION_DEFAULTS,
  type LocalTerminalPlatform,
  type LocalTerminalShellPreferences,
  type SshConnectionDefaults,
  type UiPreferences
} from '@fileterm/core'
import { t } from '../../../../i18n'
import { LOCAL_TERMINAL_SHELL_CONFIGS, localTerminalShellOptionsFor } from '../constants'
import type { SettingsModalState } from './state'

export function useSettingsPreferencesController({ state }: { state: SettingsModalState }) {
  const {
    desktopApi,
    updateStatus,
    setAutoCheckUpdates,
    autoCheckUpdates,
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
    isLoadingLocalTerminalShellOptions,
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
    connectionDefaults,
    setConnectionDefaults,
    isSavingConnectionDefaults,
    setIsSavingConnectionDefaults,
    connectionDefaultsError,
    setConnectionDefaultsError
  } = state

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

  const currentLocalTerminalPlatform: LocalTerminalPlatform | null =
    desktopApi?.platform === 'win32' || desktopApi?.platform === 'darwin' || desktopApi?.platform === 'linux'
      ? desktopApi.platform
      : null
  const currentLocalTerminalShellConfig = currentLocalTerminalPlatform
    ? (LOCAL_TERMINAL_SHELL_CONFIGS.find((config) => config.platform === currentLocalTerminalPlatform) ?? null)
    : null
  const currentLocalTerminalShellOptions = currentLocalTerminalShellConfig
    ? localTerminalShellOptionsFor(localTerminalShellOptions)
    : []

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

  const updateLocalTerminalShellDraft = (platform: LocalTerminalPlatform, value: string) => {
    setLocalTerminalShellDrafts((current) => ({ ...current, [platform]: value }))
    setLocalTerminalShellMessage(null)
    setLocalTerminalShellError(null)
  }

  const localTerminalShellsDirty = currentLocalTerminalShellConfig
    ? localTerminalShellDrafts[currentLocalTerminalShellConfig.platform] !==
      localTerminalShells[currentLocalTerminalShellConfig.platform]
    : false

  const saveLocalTerminalShells = () => {
    if (!desktopApi || !currentLocalTerminalShellConfig || isSavingLocalTerminalShells || !localTerminalShellsDirty) {
      return
    }

    const previousShells = localTerminalShells
    const platform = currentLocalTerminalShellConfig.platform
    const nextShells: Partial<LocalTerminalShellPreferences> = {
      [platform]: localTerminalShellDrafts[platform]
    }
    setLocalTerminalShellError(null)
    setLocalTerminalShellMessage(null)
    setIsSavingLocalTerminalShells(true)
    void desktopApi
      .setUiPreferences({ localTerminalShells: nextShells })
      .then((preferences) => {
        const savedShells = { ...DEFAULT_LOCAL_TERMINAL_SHELLS, ...preferences.localTerminalShells }
        setLocalTerminalShells(savedShells)
        setLocalTerminalShellDrafts(savedShells)
        setLocalTerminalShellMessage(t.localTerminalShellSaved)
      })
      .catch(() => {
        setLocalTerminalShells(previousShells)
        setLocalTerminalShellDrafts(previousShells)
        setLocalTerminalShellError(t.localTerminalShellSaveFailed)
      })
      .finally(() => setIsSavingLocalTerminalShells(false))
  }

  const setFilePanelRememberRatioPreference = (nextValue: boolean) => {
    if (!desktopApi || isSavingFilePanelPreference || nextValue === filePanelRememberRatio) {
      return
    }

    const previousValue = filePanelRememberRatio
    setFilePanelRememberRatio(nextValue)
    setFilePanelPreferenceError(null)
    setIsSavingFilePanelPreference(true)
    void desktopApi
      .setUiPreferences({ filePanelRememberRatio: nextValue })
      .then((preferences) => setFilePanelRememberRatio(preferences.filePanelRememberRatio))
      .catch(() => {
        setFilePanelRememberRatio(previousValue)
        setFilePanelPreferenceError(t.filePanelPreferenceSaveFailed)
      })
      .finally(() => setIsSavingFilePanelPreference(false))
  }

  return {
    updateStatus,
    autoCheckUpdates,
    isSavingUpdatePreference,
    updatePreferenceError,
    setUpdateCheckPreference,
    updateChannel,
    setUpdateChannelPreference,
    platformLabel,
    currentLocalTerminalPlatform,
    isLoadingLocalTerminalShellOptions,
    localTerminalShellOptions,
    setLocalTerminalShellScanVersion,
    isSavingLocalTerminalShells,
    currentLocalTerminalShellConfig,
    currentLocalTerminalShellOptions,
    localTerminalShellDrafts,
    updateLocalTerminalShellDraft,
    localTerminalShellError,
    localTerminalShellsDirty,
    saveLocalTerminalShells,
    localTerminalShellMessage,
    connectionDefaults,
    isSavingConnectionDefaults,
    setConnectionDefault,
    connectionDefaultsError,
    terminalZoomLocked,
    isSavingTerminalZoomPreference,
    terminalZoomPreferenceError,
    setTerminalZoomLockPreference,
    filePanelRememberRatio,
    isSavingFilePanelPreference,
    filePanelPreferenceError,
    setFilePanelRememberRatioPreference
  }
}
