import figLicense from '../../../terminal/autocomplete/specs/LICENSE.fig.txt?raw'
import figManifest from '../../../terminal/autocomplete/specs/index.json'
import { useExternalUrl } from '../../../../hooks/use-external-url'
import './third-party-notices.css'
import { MaterialIcon } from '../../../../components/common/material-icon/material-icon'
import { DiagnosticLogSettings } from './diagnostic-log-settings'
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

  const { openExternalUrl, externalUrlError } = useExternalUrl(desktopApi, t.openExternalUrlFailed)

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
        <h3>{t.thirdPartySoftware}</h3>
        <p>{t.figCompletionAttribution}</p>
        <p>Fig Autocomplete Specs · v{figManifest.version} · MIT</p>
        <button
          className="flat-button compact"
          type="button"
          onClick={() => void openExternalUrl('https://github.com/withfig/autocomplete')}
        >
          {t.figCompletionSource}
        </button>
        <details className="third-party-license">
          <summary>{t.thirdPartyLicense}</summary>
          <pre>{figLicense}</pre>
        </details>
        {externalUrlError ? (
          <p className="modal-error" role="alert">
            {externalUrlError}
          </p>
        ) : null}
      </section>

      <section className="settings-section">
        <h3>{t.systemLogsInfo}</h3>
        <DiagnosticLogSettings desktopApi={desktopApi} t={t} />
        <div className="logs-shortcut-card">
          <p>{t.settingsLogsDescription}</p>
          <button className="flat-button compact" onClick={onOpenLogsDirectory} type="button">
            <MaterialIcon
              name="folder_open"
              className="material-symbols-outlined"
              style={{ fontSize: '14px', marginRight: '4px', verticalAlign: 'middle' }}
            />
            {t.openLogsDirectory}
          </button>
        </div>
      </section>
    </div>
  )
}
