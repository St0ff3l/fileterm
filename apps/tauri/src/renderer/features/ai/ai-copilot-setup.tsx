import { t } from '../../i18n'

export function AiCopilotSetup({ onOpenSettings }: { onOpenSettings(): void }) {
  return (
    <section className="ai-copilot-setup" aria-labelledby="ai-copilot-setup-title">
      <span aria-hidden="true" className="material-symbols-outlined ai-copilot-setup-orb">
        auto_awesome
      </span>
      <span className="ai-copilot-eyebrow">{t.aiCopilotPreview}</span>
      <h2 id="ai-copilot-setup-title">{t.aiCopilotNotConfigured}</h2>
      <p>{t.aiCopilotNotConfiguredDescription}</p>
      <button className="ai-copilot-setup-action" type="button" onClick={onOpenSettings}>
        <span aria-hidden="true" className="material-symbols-outlined">
          tune
        </span>
        <span>{t.aiCopilotConfigureProvider}</span>
        <span aria-hidden="true" className="material-symbols-outlined ai-copilot-action-arrow">
          arrow_forward
        </span>
      </button>
      <div className="ai-copilot-principles" aria-label={t.aiCopilotPreview}>
        <article>
          <span aria-hidden="true" className="material-symbols-outlined">
            shield
          </span>
          <span>
            <strong>{t.aiCopilotPrivacyTitle}</strong>
            <small>{t.aiCopilotPrivacyDescription}</small>
          </span>
        </article>
        <article>
          <span aria-hidden="true" className="material-symbols-outlined">
            terminal
          </span>
          <span>
            <strong>{t.aiCopilotCommandTitle}</strong>
            <small>{t.aiCopilotCommandDescription}</small>
          </span>
        </article>
      </div>
    </section>
  )
}
