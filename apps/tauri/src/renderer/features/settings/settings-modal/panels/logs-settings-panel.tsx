import type { FileTermDesktopApi } from '@fileterm/core'
import { MaterialIcon } from '../../../../components/common/material-icon/material-icon'
import type { LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import { DiagnosticLogSettings } from './diagnostic-log-settings'
import './diagnostic-log-settings.css'

export function LogsSettingsPanel() {
  const { t, desktopApi, onOpenLogsDirectory } = useSettingsModalContext<{
    t: LocaleMessages
    desktopApi: FileTermDesktopApi | undefined
    onOpenLogsDirectory(): void
  }>()

  return (
    <div className="settings-panel logs-settings-panel">
      <section className="settings-section">
        <h3>{t.logsDirectory}</h3>
        <div className="logs-shortcut-card">
          <p>{t.settingsLogsDescription}</p>
          <button className="flat-button compact" onClick={onOpenLogsDirectory} type="button">
            <MaterialIcon name="folder_open" />
            {t.openLogsDirectory}
          </button>
        </div>
      </section>

      <section className="settings-section">
        <h3>{t.logRecording}</h3>
        <DiagnosticLogSettings desktopApi={desktopApi} t={t} />
      </section>
    </div>
  )
}
