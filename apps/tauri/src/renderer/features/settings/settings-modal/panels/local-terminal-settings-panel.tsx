import type { Dispatch, SetStateAction } from 'react'
import type {
  FileTermDesktopApi,
  LocalTerminalPlatform,
  LocalTerminalShellOption,
  LocalTerminalShellPreferences
} from '@fileterm/core'
import { AppIcon } from '../../../common/app-icon'
import { DropdownSelect } from '../../../common/dropdown-select'
import { StableButtonContent } from '../../../common/stable-button-content'
import { formatMessage, type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type LocalTerminalShellConfig = {
  platform: LocalTerminalPlatform
  labelKey: keyof LocaleMessages
  hintKey: keyof LocaleMessages
  placeholder: string
}

type LocalTerminalSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  currentLocalTerminalPlatform: LocalTerminalPlatform | null
  isLoadingLocalTerminalShellOptions: boolean
  localTerminalShellOptions: LocalTerminalShellOption[]
  setLocalTerminalShellScanVersion: Dispatch<SetStateAction<number>>
  isSavingLocalTerminalShells: boolean
  currentLocalTerminalShellConfig: LocalTerminalShellConfig | null
  currentLocalTerminalShellOptions: Array<{ value: string; label: string }>
  localTerminalShellDrafts: LocalTerminalShellPreferences
  updateLocalTerminalShellDraft(platform: LocalTerminalPlatform, value: string): void
  localTerminalShellError: string | null
  localTerminalShellsDirty: boolean
  saveLocalTerminalShells(): void
  localTerminalShellMessage: string | null
}

export function LocalTerminalSettingsPanel() {
  const {
    t,
    desktopApi,
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
    localTerminalShellMessage
  } = useSettingsModalContext<LocalTerminalSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.localTerminalSettings}</h3>
        <p className="settings-tools-hint">{t.localTerminalSettingsHint}</p>

        <div className="local-terminal-shell-detection">
          <div className="local-terminal-shell-detection-copy">
            <strong>
              {currentLocalTerminalPlatform
                ? isLoadingLocalTerminalShellOptions
                  ? t.localTerminalShellDetecting
                  : formatMessage(t.localTerminalShellDetectionSummary, {
                      count: localTerminalShellOptions.length
                    })
                : t.localTerminalShellDetectionDesktopOnly}
            </strong>
            <p>{t.localTerminalShellDetectionHint}</p>
          </div>
          <button
            className="flat-button compact"
            disabled={!desktopApi || isLoadingLocalTerminalShellOptions || isSavingLocalTerminalShells}
            onClick={() => setLocalTerminalShellScanVersion((version) => version + 1)}
            type="button"
          >
            <AppIcon name="refresh" size={14} />
            {t.localTerminalShellDetect}
          </button>
        </div>

        <div className="local-terminal-shell-list">
          {currentLocalTerminalShellConfig ? (
            <div className="local-terminal-shell-card">
              <div className="local-terminal-shell-card-heading">
                <div>
                  <strong>{t[currentLocalTerminalShellConfig.labelKey]}</strong>
                  <p>{t[currentLocalTerminalShellConfig.hintKey]}</p>
                </div>
                <span className="local-terminal-shell-current-badge">{t.localTerminalShellCurrentPlatform}</span>
              </div>
              <div className="local-terminal-shell-control-row">
                <label className="local-terminal-shell-input-label">
                  <span>{t.localTerminalShellExecutable}</span>
                  <input
                    aria-label={`${t[currentLocalTerminalShellConfig.labelKey]} · ${t.localTerminalShellExecutable}`}
                    disabled={!desktopApi || isSavingLocalTerminalShells}
                    onChange={(event) =>
                      updateLocalTerminalShellDraft(currentLocalTerminalShellConfig.platform, event.target.value)
                    }
                    placeholder={currentLocalTerminalShellConfig.placeholder}
                    spellCheck={false}
                    value={localTerminalShellDrafts[currentLocalTerminalShellConfig.platform]}
                  />
                </label>
                <DropdownSelect
                  ariaLabel={`${t[currentLocalTerminalShellConfig.labelKey]} ${t.localTerminalShellSelectPlaceholder}`}
                  className="local-terminal-shell-select"
                  disabled={!desktopApi || isSavingLocalTerminalShells || currentLocalTerminalShellOptions.length === 0}
                  onChange={(value) => updateLocalTerminalShellDraft(currentLocalTerminalShellConfig.platform, value)}
                  options={currentLocalTerminalShellOptions}
                  placeholder={t.localTerminalShellSelectPlaceholder}
                  value={localTerminalShellDrafts[currentLocalTerminalShellConfig.platform]}
                />
              </div>
              <p className="local-terminal-shell-input-hint">{t.localTerminalShellExecutableHint}</p>
              <button
                className="flat-button compact local-terminal-shell-reset"
                disabled={!desktopApi || isSavingLocalTerminalShells}
                onClick={() => updateLocalTerminalShellDraft(currentLocalTerminalShellConfig.platform, '')}
                type="button"
              >
                <AppIcon name="refresh" size={13} />
                {t.localTerminalShellReset}
              </button>
            </div>
          ) : (
            <p className="local-terminal-shell-empty">{t.localTerminalShellDetectionDesktopOnly}</p>
          )}
        </div>

        {localTerminalShellError ? <p className="modal-error">{localTerminalShellError}</p> : null}
        <div className="local-terminal-shell-actions">
          <button
            aria-busy={isSavingLocalTerminalShells}
            className="primary-button compact"
            disabled={!desktopApi || isSavingLocalTerminalShells || !localTerminalShellsDirty}
            onClick={saveLocalTerminalShells}
            type="button"
          >
            <StableButtonContent
              busy={isSavingLocalTerminalShells}
              icon={<AppIcon name="disk" size={14} />}
              label={t.localTerminalShellSave}
            />
          </button>
          <span
            aria-live="polite"
            className={`local-terminal-shell-save-message${localTerminalShellMessage ? ' is-visible' : ''}`}
          >
            {localTerminalShellMessage ?? ''}
          </span>
        </div>
      </section>
    </div>
  )
}
