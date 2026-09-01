import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type LanguageSettingsPanelContext = {
  t: LocaleMessages
  locale: 'zhCN' | 'enUS'
  onSetLocale(value: 'zhCN' | 'enUS'): void
}

export function LanguageSettingsPanel() {
  const { t, locale, onSetLocale } = useSettingsModalContext<LanguageSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.languageSelection}</h3>
        <div className="language-selector-row">
          <button
            className={`lang-card ${locale === 'zhCN' ? 'active' : ''}`}
            onClick={() => onSetLocale('zhCN')}
            type="button"
          >
            {t.languageZhCN}
          </button>
          <button
            className={`lang-card ${locale === 'enUS' ? 'active' : ''}`}
            onClick={() => onSetLocale('enUS')}
            type="button"
          >
            {t.languageEnglish}
          </button>
        </div>
      </section>
    </div>
  )
}
