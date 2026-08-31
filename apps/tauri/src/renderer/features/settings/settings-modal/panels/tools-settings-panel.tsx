import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type ToolsSettingsPanelContext = {
  t: LocaleMessages
  managerToolsHint: string
  managerToolsActionLabel: string
  onOpenConnectionManager(): void
  onOpenCommandManager(): void
}

export function ToolsSettingsPanel() {
  const { t, managerToolsHint, managerToolsActionLabel, onOpenConnectionManager, onOpenCommandManager } =
    useSettingsModalContext<ToolsSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.managerToolsShortcut}</h3>
        <p className="settings-tools-hint">{managerToolsHint}</p>
        <div className="tools-shortcuts-grid">
          <div className="tool-shortcut-card">
            <span className="material-symbols-outlined tool-card-icon">settings_ethernet</span>
            <div className="tool-card-details">
              <strong>{t.connectionManager}</strong>
              <p>{t.settingsConnectionManagerDescription}</p>
              <button className="primary-button compact" onClick={onOpenConnectionManager} type="button">
                {managerToolsActionLabel}
              </button>
            </div>
          </div>
          <div className="tool-shortcut-card">
            <span className="material-symbols-outlined tool-card-icon">terminal</span>
            <div className="tool-card-details">
              <strong>{t.commandManager}</strong>
              <p>{t.settingsCommandManagerDescription}</p>
              <button className="primary-button compact" onClick={onOpenCommandManager} type="button">
                {managerToolsActionLabel}
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>
  )
}
