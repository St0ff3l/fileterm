import type { Dispatch, SetStateAction, PointerEvent as ReactPointerEvent } from 'react'
import type {
  FileTermDesktopApi,
  ImportedFont,
  OverviewSectionId,
  SavedTheme,
  TerminalAnsiColorName,
  ThemeConfig
} from '@fileterm/core'
import { AppIcon } from '../../../common/app-icon'
import { managerDropClass } from '../../../common/manager-drag'
import { targetsNestedManagerControl } from '../../../common/manager-interactions'
import { ConfirmActionDialog } from '../../../common/confirm-action-dialog'
import { DropdownSelect } from '../../../common/dropdown-select'
import { SelectionControl } from '../../../common/selection-control'
import { StableButtonContent } from '../../../common/stable-button-content'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import {
  ANSI_COLOR_LABELS,
  ANSI_COLOR_NAMES,
  THEME_PRESETS,
  ThemeColorField,
  type ThemePresetVariant
} from '../constants'

type InterfaceSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  themeConfig: ThemeConfig
  normalizedThemeConfig: ThemeConfig
  customThemes: SavedTheme[]
  themeConfigOperation: 'import' | 'copy' | null
  importThemeConfig(): Promise<void>
  copyThemeConfig(): Promise<void>
  themeConfigMessage: { text: string; kind: 'success' | 'error' | 'warning' } | null
  themePresetValue: string
  themePresetLabel: string
  themePresetCode: string
  applyThemePreset(presetId: string, variant?: ThemePresetVariant): void
  switchThemeVariant(nextVariant: ThemePresetVariant): void
  customThemeName: string
  setCustomThemeName: Dispatch<SetStateAction<string>>
  saveCustomTheme(): void
  selectedSavedTheme: SavedTheme | undefined
  editingSavedTheme: SavedTheme | undefined
  showDeleteThemeConfirm: boolean
  setShowDeleteThemeConfirm: Dispatch<SetStateAction<boolean>>
  deleteCustomTheme(): void
  importedFonts: ImportedFont[]
  fontImportKind: 'ui' | 'code' | null
  importFontFor(kind: 'ui' | 'code'): Promise<void>
  fontToDelete: ImportedFont | null
  setFontToDelete: Dispatch<SetStateAction<ImportedFont | null>>
  handleDeleteFont(font: ImportedFont): Promise<void>
  fontImportError: string | null
  updateThemeBody(patch: Partial<ThemeConfig['theme']>): void
  updateThemeSemanticColors(patch: Partial<ThemeConfig['theme']['semanticColors']>): void
  updateThemeFonts(patch: Partial<ThemeConfig['theme']['fonts']>): void
  updateTerminalTheme(patch: Partial<ThemeConfig['theme']['terminal']>): void
  updateTerminalAnsiColor(name: TerminalAnsiColorName, value: string): void
  updateTerminalSearchColors(patch: Partial<ThemeConfig['theme']['terminal']['search']>): void
  terminalZoomLocked: boolean
  isSavingTerminalZoomPreference: boolean
  terminalZoomPreferenceError: string | null
  setTerminalZoomLockPreference(nextValue: boolean): void
  filePanelRememberRatio: boolean
  isSavingFilePanelPreference: boolean
  filePanelPreferenceError: string | null
  setFilePanelRememberRatioPreference(nextValue: boolean): void
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  overviewSectionMeta: Record<OverviewSectionId, { title: string; hint: string }>
  draggingOverviewSection: OverviewSectionId | null
  dragOverOverviewSection: OverviewSectionId | null
  overviewDragPosition: 'top' | 'bottom' | 'inside' | null
  isSavingOverviewPreference: boolean
  overviewPreferenceError: string | null
  suppressOverviewCardClickRef: { current: boolean }
  setOverviewShowStatsPreference(nextValue: boolean): void
  setOverviewShowRecentPreference(nextValue: boolean): void
  setOverviewShowAllConnectionsPreference(nextValue: boolean): void
  setOverviewShowQuickActionsPreference(nextValue: boolean): void
  handleOverviewPointerDown(event: ReactPointerEvent<HTMLElement>, source: OverviewSectionId): void
}

export function InterfaceSettingsPanel() {
  const {
    t,
    desktopApi,
    themeConfig,
    normalizedThemeConfig,
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
    importedFonts,
    fontImportKind,
    importFontFor,
    fontToDelete,
    setFontToDelete,
    handleDeleteFont,
    fontImportError,
    updateThemeBody,
    updateThemeSemanticColors,
    updateThemeFonts,
    updateTerminalTheme,
    updateTerminalAnsiColor,
    updateTerminalSearchColors,
    terminalZoomLocked,
    isSavingTerminalZoomPreference,
    terminalZoomPreferenceError,
    setTerminalZoomLockPreference,
    filePanelRememberRatio,
    isSavingFilePanelPreference,
    filePanelPreferenceError,
    setFilePanelRememberRatioPreference,
    overviewShowStats,
    overviewShowRecent,
    overviewShowAllConnections,
    overviewShowQuickActions,
    overviewSectionOrder,
    overviewSectionMeta,
    draggingOverviewSection,
    dragOverOverviewSection,
    overviewDragPosition,
    isSavingOverviewPreference,
    overviewPreferenceError,
    suppressOverviewCardClickRef,
    setOverviewShowStatsPreference,
    setOverviewShowRecentPreference,
    setOverviewShowAllConnectionsPreference,
    setOverviewShowQuickActionsPreference,
    handleOverviewPointerDown
  } = useSettingsModalContext<InterfaceSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.appearanceTheme}</h3>
        <div aria-label={t.themeSelection} className="theme-options-grid" role="group">
          <button
            aria-pressed={themeConfig.variant === 'light'}
            className={`theme-card light ${themeConfig.variant === 'light' ? 'active' : ''}`}
            onClick={() => switchThemeVariant('light')}
            type="button"
          >
            <div aria-hidden="true" className="theme-card-preview">
              <div className="preview-header">
                <span className="dot dot-close" />
                <span className="dot dot-min" />
                <span className="dot dot-max" />
              </div>
              <div className="preview-body">
                <div className="preview-sidebar"></div>
                <div className="preview-content"></div>
              </div>
            </div>
            <span className="theme-card-label">{t.themeLight}</span>
          </button>
          <button
            aria-pressed={themeConfig.variant === 'dark'}
            className={`theme-card dark ${themeConfig.variant === 'dark' ? 'active' : ''}`}
            onClick={() => switchThemeVariant('dark')}
            type="button"
          >
            <div aria-hidden="true" className="theme-card-preview">
              <div className="preview-header">
                <span className="dot dot-close" />
                <span className="dot dot-min" />
                <span className="dot dot-max" />
              </div>
              <div className="preview-body">
                <div className="preview-sidebar"></div>
                <div className="preview-content"></div>
              </div>
            </div>
            <span className="theme-card-label">{t.themeDark}</span>
          </button>
        </div>
      </section>

      <section className="settings-section theme-config-section">
        <div className="theme-config-heading">
          <div>
            <h3>{t.themeCustomization}</h3>
            <p className="settings-tools-hint">{t.themeCustomizationHint}</p>
          </div>
          <div className="theme-config-action-group">
            <div className="theme-config-actions">
              <button
                aria-busy={themeConfigOperation === 'import'}
                className="flat-button compact theme-config-action-button"
                disabled={themeConfigOperation !== null}
                onClick={() => void importThemeConfig()}
                type="button"
              >
                <StableButtonContent
                  busy={themeConfigOperation === 'import'}
                  busyLabel={t.themeWorking}
                  icon={<AppIcon name="download" size={14} />}
                  label={t.themeImport}
                />
              </button>
              <button
                aria-busy={themeConfigOperation === 'copy'}
                className="flat-button compact theme-config-action-button"
                disabled={themeConfigOperation !== null}
                onClick={() => void copyThemeConfig()}
                type="button"
              >
                <StableButtonContent
                  busy={themeConfigOperation === 'copy'}
                  busyLabel={t.themeWorking}
                  icon={<AppIcon name="copy" size={14} />}
                  label={t.themeCopy}
                />
              </button>
            </div>
            <span
              aria-live="polite"
              className={`theme-config-action-status${themeConfigMessage ? ` is-visible is-${themeConfigMessage.kind}` : ''}`}
            >
              {themeConfigMessage?.text ?? ''}
            </span>
          </div>
        </div>

        <div className="theme-config-toolbar">
          <div className="theme-config-preset-control">
            <span className="theme-config-label">{t.themePreset}</span>
            <DropdownSelect
              ariaLabel={t.themePreset}
              className="theme-config-select"
              onChange={applyThemePreset}
              options={[
                ...THEME_PRESETS.map((preset) => ({ value: preset.id, label: t[preset.labelKey] })),
                ...customThemes.map((savedTheme) => ({
                  value: `saved:${savedTheme.id}`,
                  label: savedTheme.name
                })),
                { value: 'custom', label: t.themePresetCustom }
              ]}
              value={themePresetValue}
            />
          </div>
          <div className="theme-config-name-control">
            <span className="theme-config-label">{t.themeCustomName}</span>
            <input
              aria-label={t.themeCustomName}
              className="theme-config-name-input"
              maxLength={128}
              onChange={(event) => setCustomThemeName(event.target.value)}
              placeholder={t.themeCustomNamePlaceholder}
              value={customThemeName}
            />
          </div>
          <div className="theme-config-actions-control">
            <button className="primary-button compact theme-config-save-button" onClick={saveCustomTheme} type="button">
              <StableButtonContent
                icon={<AppIcon name={editingSavedTheme ? 'check' : 'edit'} size={14} />}
                label={editingSavedTheme ? t.themeUpdate : t.themeSave}
                reserveLabel={editingSavedTheme ? t.themeSave : t.themeUpdate}
              />
            </button>
            {selectedSavedTheme ? (
              <button
                className="theme-config-danger-button"
                onClick={() => setShowDeleteThemeConfirm(true)}
                type="button"
              >
                <AppIcon name="trash" size={14} />
                {t.themeDelete}
              </button>
            ) : null}
          </div>
          <div
            className="theme-config-preview"
            style={{
              backgroundColor: themeConfig.theme.surface,
              borderColor: 'var(--border-light)',
              color: themeConfig.theme.ink
            }}
          >
            <div className="theme-config-preview-title">
              <span style={{ color: themeConfig.theme.accent }}>●</span>
              <span>{themePresetLabel}</span>
            </div>
            <code className="theme-config-preview-code">
              <span style={{ color: themeConfig.theme.semanticColors.keyword }}>const</span>{' '}
              <span style={{ color: themeConfig.theme.semanticColors.skill }}>theme</span>
              {' = '}
              <span style={{ color: themeConfig.theme.accent }}>{themePresetCode}</span>
            </code>
          </div>
        </div>

        <div className="theme-config-color-groups">
          <section className="theme-config-color-group">
            <div className="theme-config-section-heading">
              <h4>{t.themeBaseColors}</h4>
              <span>{t.themeBaseColorsHint}</span>
            </div>
            <div className="theme-config-fields">
              <ThemeColorField
                label={t.themePrimaryColor}
                onChange={(value) => updateThemeBody({ accent: value })}
                value={normalizedThemeConfig.theme.accent}
              />
              <ThemeColorField
                label={t.themeSecondaryColor}
                onChange={(value) => updateThemeSemanticColors({ secondary: value })}
                value={normalizedThemeConfig.theme.semanticColors.secondary}
              />
              <ThemeColorField
                label={t.themeSurfaceColor}
                onChange={(value) => updateThemeBody({ surface: value })}
                value={normalizedThemeConfig.theme.surface}
              />
              <ThemeColorField
                label={t.themeSurfaceSecondaryColor}
                onChange={(value) => updateThemeBody({ surfaceSecondary: value })}
                value={normalizedThemeConfig.theme.surfaceSecondary}
              />
              <ThemeColorField
                label={t.themeSurfaceElevatedColor}
                onChange={(value) => updateThemeBody({ surfaceElevated: value })}
                value={normalizedThemeConfig.theme.surfaceElevated}
              />
              <ThemeColorField
                label={t.themeTextPrimaryColor}
                onChange={(value) => updateThemeBody({ ink: value })}
                value={normalizedThemeConfig.theme.ink}
              />
              <ThemeColorField
                label={t.themeTextSecondaryColor}
                onChange={(value) => updateThemeSemanticColors({ textSecondary: value })}
                value={normalizedThemeConfig.theme.semanticColors.textSecondary}
              />
              <ThemeColorField
                label={t.themeTotalColor}
                onChange={(value) => updateThemeSemanticColors({ total: value })}
                value={normalizedThemeConfig.theme.semanticColors.total}
              />
              <ThemeColorField
                label={t.themeTelnetColor}
                onChange={(value) => updateThemeSemanticColors({ telnet: value })}
                value={normalizedThemeConfig.theme.semanticColors.telnet}
              />
              <ThemeColorField
                label={t.themeFtpColor}
                onChange={(value) => updateThemeSemanticColors({ ftp: value })}
                value={normalizedThemeConfig.theme.semanticColors.ftp}
              />
              <ThemeColorField
                label={t.themeNetworkRxColor}
                onChange={(value) => updateThemeSemanticColors({ networkRx: value })}
                value={normalizedThemeConfig.theme.semanticColors.networkRx}
              />
              <ThemeColorField
                label={t.themeNetworkTxColor}
                onChange={(value) => updateThemeSemanticColors({ networkTx: value })}
                value={normalizedThemeConfig.theme.semanticColors.networkTx}
              />
            </div>
          </section>

          <section className="theme-config-color-group">
            <div className="theme-config-section-heading">
              <h4>{t.themeStatusColors}</h4>
              <span>{t.themeStatusColorsHint}</span>
            </div>
            <div className="theme-config-fields">
              <ThemeColorField
                label={t.themeInfoColor}
                onChange={(value) => updateThemeSemanticColors({ info: value })}
                value={normalizedThemeConfig.theme.semanticColors.info}
              />
              <ThemeColorField
                label={t.themeWarningColor}
                onChange={(value) => updateThemeSemanticColors({ warning: value })}
                value={normalizedThemeConfig.theme.semanticColors.warning}
              />
              <ThemeColorField
                label={t.themeErrorColor}
                onChange={(value) => updateThemeSemanticColors({ error: value })}
                value={normalizedThemeConfig.theme.semanticColors.error}
              />
              <ThemeColorField
                label={t.themeSuccessColor}
                onChange={(value) => updateThemeSemanticColors({ success: value })}
                value={normalizedThemeConfig.theme.semanticColors.success}
              />
            </div>
          </section>
        </div>

        <div className="theme-config-font-grid">
          <div className="theme-config-control">
            <span className="theme-config-label">{t.themeUiFont}</span>
            <div className="theme-config-font-control-row">
              <DropdownSelect
                ariaLabel={t.themeUiFont}
                className="theme-config-select"
                onChange={(value) => updateThemeFonts({ ui: value || null })}
                options={[
                  { value: '', label: t.themeSystemDefault },
                  { value: 'Inter', label: 'Inter' },
                  { value: 'SF Pro Text', label: 'SF Pro Text' },
                  { value: 'Noto Sans SC', label: 'Noto Sans SC' },
                  ...importedFonts.map((font) => ({
                    value: font.family,
                    label: `${font.family} (${font.format.toUpperCase()})`
                  }))
                ]}
                value={themeConfig.theme.fonts.ui ?? ''}
              />
              <button
                aria-label={t.themeImportFont}
                aria-busy={fontImportKind === 'ui'}
                className="flat-button compact theme-font-import-button"
                disabled={!desktopApi || fontImportKind !== null}
                onClick={() => void importFontFor('ui')}
                title={t.themeImportFont}
                type="button"
              >
                <StableButtonContent
                  busy={fontImportKind === 'ui'}
                  busyLabel={t.themeImportingFont}
                  icon={<AppIcon name="upload" size={14} />}
                  label={t.themeImportFont}
                />
              </button>
            </div>
          </div>
          <div className="theme-config-control">
            <span className="theme-config-label">{t.themeCodeFont}</span>
            <div className="theme-config-font-control-row">
              <DropdownSelect
                ariaLabel={t.themeCodeFont}
                className="theme-config-select"
                onChange={(value) => updateThemeFonts({ code: value || null })}
                options={[
                  { value: '', label: t.themeSystemDefault },
                  { value: 'JetBrains Mono', label: 'JetBrains Mono' },
                  { value: 'SF Mono', label: 'SF Mono' },
                  { value: 'Cascadia Code', label: 'Cascadia Code' },
                  ...importedFonts.map((font) => ({
                    value: font.family,
                    label: `${font.family} (${font.format.toUpperCase()})`
                  }))
                ]}
                value={themeConfig.theme.fonts.code ?? ''}
              />
              <button
                aria-label={t.themeImportFont}
                aria-busy={fontImportKind === 'code'}
                className="flat-button compact theme-font-import-button"
                disabled={!desktopApi || fontImportKind !== null}
                onClick={() => void importFontFor('code')}
                title={t.themeImportFont}
                type="button"
              >
                <StableButtonContent
                  busy={fontImportKind === 'code'}
                  busyLabel={t.themeImportingFont}
                  icon={<AppIcon name="upload" size={14} />}
                  label={t.themeImportFont}
                />
              </button>
            </div>
          </div>
        </div>
        {fontImportError ? <p className="settings-tools-error">{fontImportError}</p> : null}

        {importedFonts.length > 0 ? (
          <div className="theme-config-imported-fonts">
            <div className="theme-config-imported-fonts-header">
              <span className="theme-config-imported-fonts-title">{t.themeImportedFonts}</span>
              <span className="theme-config-imported-fonts-count">{importedFonts.length}</span>
            </div>
            <div className="theme-config-imported-fonts-list">
              {importedFonts.map((font) => (
                <div key={font.id} className="theme-config-imported-font-item">
                  <span className="theme-config-imported-font-name" title={font.fileName}>
                    {font.family}
                    <span className="theme-config-imported-font-format">{font.format.toUpperCase()}</span>
                  </span>
                  <button
                    type="button"
                    className="flat-button compact danger theme-config-font-delete-btn"
                    title={`${t.themeDeleteFont}: ${font.family}`}
                    aria-label={`${t.themeDeleteFont}: ${font.family}`}
                    onClick={() => setFontToDelete(font)}
                  >
                    <AppIcon name="trash" size={13} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <details className="theme-config-subsection theme-advanced-section">
          <summary className="theme-config-section-summary">
            <span className="theme-config-section-summary-copy">
              <strong>{t.themeSemanticColors}</strong>
              <span>{t.themeAdvancedHint}</span>
            </span>
          </summary>
          <div className="theme-config-fields">
            <ThemeColorField
              label={t.themeDiffAdded}
              onChange={(value) => updateThemeSemanticColors({ diffAdded: value })}
              value={normalizedThemeConfig.theme.semanticColors.diffAdded}
            />
            <ThemeColorField
              label={t.themeDiffRemoved}
              onChange={(value) => updateThemeSemanticColors({ diffRemoved: value })}
              value={normalizedThemeConfig.theme.semanticColors.diffRemoved}
            />
            <ThemeColorField
              label={t.themeSkillColor}
              onChange={(value) => updateThemeSemanticColors({ skill: value })}
              value={normalizedThemeConfig.theme.semanticColors.skill}
            />
            <ThemeColorField
              label={t.themeKeywordColor}
              onChange={(value) => updateThemeSemanticColors({ keyword: value })}
              value={normalizedThemeConfig.theme.semanticColors.keyword}
            />
          </div>
        </details>

        <div className="theme-config-subsection">
          <h4>{t.themeTerminalColors}</h4>
          <div className="theme-config-fields">
            <ThemeColorField
              label={t.themeTerminalBackground}
              onChange={(value) => updateTerminalTheme({ background: value })}
              value={themeConfig.theme.terminal.background}
            />
            <ThemeColorField
              label={t.themeTerminalForeground}
              onChange={(value) => updateTerminalTheme({ foreground: value })}
              value={themeConfig.theme.terminal.foreground}
            />
            <ThemeColorField
              label={t.themeTerminalCursor}
              onChange={(value) => updateTerminalTheme({ cursor: value })}
              value={themeConfig.theme.terminal.cursor}
            />
            <ThemeColorField
              label={t.themeTerminalCursorAccent}
              onChange={(value) => updateTerminalTheme({ cursorAccent: value })}
              value={themeConfig.theme.terminal.cursorAccent}
            />
            <ThemeColorField
              label={t.themeTerminalSelection}
              onChange={(value) => updateTerminalTheme({ selectionBackground: value })}
              value={themeConfig.theme.terminal.selectionBackground}
            />
            <ThemeColorField
              label={t.themeTerminalSelectionText}
              onChange={(value) => updateTerminalTheme({ selectionForeground: value })}
              value={themeConfig.theme.terminal.selectionForeground}
            />
          </div>
          <div className="theme-config-fields theme-config-search-fields">
            <ThemeColorField
              label={t.themeSearchMatch}
              onChange={(value) => updateTerminalSearchColors({ matchBackground: value })}
              value={themeConfig.theme.terminal.search.matchBackground}
            />
            <ThemeColorField
              label={t.themeSearchActiveMatch}
              onChange={(value) => updateTerminalSearchColors({ activeMatchBackground: value })}
              value={themeConfig.theme.terminal.search.activeMatchBackground}
            />
            <ThemeColorField
              label={t.themeSearchActiveText}
              onChange={(value) => updateTerminalSearchColors({ activeMatchText: value })}
              value={themeConfig.theme.terminal.search.activeMatchText}
            />
            <ThemeColorField
              label={t.themeSearchBorder}
              onChange={(value) => updateTerminalSearchColors({ activeMatchBorder: value })}
              value={themeConfig.theme.terminal.search.activeMatchBorder}
            />
          </div>
        </div>

        <details className="theme-advanced-section theme-terminal-ansi-details">
          <summary className="theme-config-section-summary">
            <span className="theme-config-section-summary-copy">
              <strong>{t.themeAnsiColors}</strong>
              <span>{t.themeAnsiHint}</span>
            </span>
            <span className="theme-config-section-summary-count">16</span>
          </summary>
          <div className="theme-terminal-ansi-groups">
            <section className="theme-terminal-ansi-group">
              <h5>{t.themeAnsiNormal}</h5>
              <div className="theme-config-fields">
                {ANSI_COLOR_NAMES.slice(0, 8).map((name) => (
                  <ThemeColorField
                    key={name}
                    label={ANSI_COLOR_LABELS[name]}
                    onChange={(value) => updateTerminalAnsiColor(name, value)}
                    value={themeConfig.theme.terminal.ansi[name]}
                  />
                ))}
              </div>
            </section>
            <section className="theme-terminal-ansi-group">
              <h5>{t.themeAnsiBright}</h5>
              <div className="theme-config-fields">
                {ANSI_COLOR_NAMES.slice(8).map((name) => (
                  <ThemeColorField
                    key={name}
                    label={ANSI_COLOR_LABELS[name]}
                    onChange={(value) => updateTerminalAnsiColor(name, value)}
                    value={themeConfig.theme.terminal.ansi[name]}
                  />
                ))}
              </div>
            </section>
          </div>
        </details>

        {showDeleteThemeConfirm && selectedSavedTheme ? (
          <ConfirmActionDialog
            confirmLabel={t.delete}
            confirmVariant="danger"
            description={t.themeDeleteConfirmDescription.replace('{name}', selectedSavedTheme.name)}
            onClose={() => setShowDeleteThemeConfirm(false)}
            onConfirm={deleteCustomTheme}
            title={t.themeDeleteConfirmTitle}
          />
        ) : null}

        {fontToDelete ? (
          <ConfirmActionDialog
            confirmLabel={t.delete}
            confirmVariant="danger"
            description={t.themeDeleteFontConfirm.replace('{name}', fontToDelete.family)}
            onClose={() => setFontToDelete(null)}
            onConfirm={() => void handleDeleteFont(fontToDelete)}
            title={t.themeDeleteFont}
          />
        ) : null}
      </section>

      <section className="settings-section">
        <h3>{t.terminalDisplaySettings}</h3>
        <p className="settings-tools-hint">{t.terminalDisplaySettingsHint}</p>
        <div className="overview-preference-list">
          <label className="overview-preference-row">
            <span className="overview-preference-copy">
              <strong>{t.lockTerminalZoom}</strong>
              <p>{t.lockTerminalZoomHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={terminalZoomLocked}
                disabled={!desktopApi || isSavingTerminalZoomPreference}
                onChange={(event) => setTerminalZoomLockPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
        </div>
        {terminalZoomPreferenceError ? <p className="modal-error">{terminalZoomPreferenceError}</p> : null}
      </section>

      <section className="settings-section">
        <h3>{t.filePanelSettings}</h3>
        <p className="settings-tools-hint">{t.filePanelSettingsHint}</p>
        <div className="overview-preference-list">
          <label className="overview-preference-row">
            <span className="overview-preference-copy">
              <strong>{t.rememberFilePanelRatio}</strong>
              <p>{t.rememberFilePanelRatioHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={filePanelRememberRatio}
                disabled={!desktopApi || isSavingFilePanelPreference}
                onChange={(event) => setFilePanelRememberRatioPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
        </div>
        {filePanelPreferenceError ? <p className="modal-error">{filePanelPreferenceError}</p> : null}
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
                  <SelectionControl
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
  )
}
