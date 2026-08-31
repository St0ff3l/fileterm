import type { FileTermDesktopApi } from '@fileterm/core'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type SystemSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  platformLabel: string
  onOpenLogsDirectory(): void
}

export function SystemSettingsPanel() {
  const { t, desktopApi, platformLabel, onOpenLogsDirectory } = useSettingsModalContext<SystemSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.aboutAppInfo}</h3>
        <div className="about-info-list">
          <div className="about-info-item">
            <span className="info-label">{t.versionLabel}</span>
            <span className="info-value">v{desktopApi?.appVersion ?? '—'}</span>
          </div>
          <div className="about-info-item">
            <span className="info-label">{desktopApi?.runtimeName ?? '—'}</span>
            <span className="info-value">v{desktopApi?.runtimeVersion ?? '—'}</span>
          </div>
          <div className="about-info-item">
            <span className="info-label">{t.environmentInfo}</span>
            <span className="info-value">{platformLabel}</span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h3>{t.systemLogsInfo}</h3>
        <div className="logs-shortcut-card">
          <p>{t.settingsLogsDescription}</p>
          <button className="flat-button compact" onClick={onOpenLogsDirectory} type="button">
            <span
              className="material-symbols-outlined"
              style={{ fontSize: '14px', marginRight: '4px', verticalAlign: 'middle' }}
            >
              folder_open
            </span>
            {t.openLogsDirectory}
          </button>
        </div>
      </section>
    </div>
  )
}
