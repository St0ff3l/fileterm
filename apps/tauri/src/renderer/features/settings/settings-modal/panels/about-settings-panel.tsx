import type { FileTermDesktopApi } from '@fileterm/core'
import type { LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import { OpenSourceNotices } from './open-source-notices'
import './about-settings-panel.css'

export function AboutSettingsPanel() {
  const { t, desktopApi, platformLabel } = useSettingsModalContext<{
    t: LocaleMessages
    desktopApi: FileTermDesktopApi | undefined
    platformLabel: string
  }>()

  return (
    <div className="settings-panel about-settings-panel">
      <section className="settings-section">
        <h3>{t.aboutAppInfo}</h3>
        <dl className="about-info-list">
          <div>
            <dt>{t.versionLabel}</dt>
            <dd>{desktopApi?.appVersion ? `v${desktopApi.appVersion}` : '—'}</dd>
          </div>
          <div>
            <dt>{desktopApi?.runtimeName ?? 'Tauri'}</dt>
            <dd>{desktopApi?.runtimeVersion ? `v${desktopApi.runtimeVersion}` : '—'}</dd>
          </div>
          <div>
            <dt>{t.environmentInfo}</dt>
            <dd>{platformLabel}</dd>
          </div>
        </dl>
      </section>

      <section className="settings-section">
        <h3>{t.thirdPartySoftware}</h3>
        <OpenSourceNotices desktopApi={desktopApi} t={t} />
      </section>
    </div>
  )
}
