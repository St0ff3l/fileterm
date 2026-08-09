import { useEffect, useState } from 'react'
import type { AiProviderSummary, ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

export function AiCopilotPanel({
  activeProfile,
  activeSession,
  activeTab,
  onClose,
  onOpenSettings
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  onClose(): void
  onOpenSettings(): void
}) {
  const isSshTarget = activeTab?.sessionType === 'ssh' && Boolean(activeSession)
  const host = activeSession?.accessHost || activeProfile?.host || activeTab?.title || t.aiCopilotNoTerminalTitle
  const user = activeSession?.shellUser || activeSession?.loginUser || activeProfile?.username
  const targetLabel = isSshTarget && user ? `${user}@${host}` : host
  const workingDirectory = activeSession?.shellCwd || activeSession?.remotePath || '~'
  const [activeProvider, setActiveProvider] = useState<AiProviderSummary | null>(null)

  useEffect(() => {
    let canceled = false
    const desktopApi = window.fileterm
    if (!desktopApi) return

    void desktopApi
      .listAiProviders()
      .then((providers) => {
        if (canceled) return
        setActiveProvider(providers.find((provider) => provider.isDefault && provider.usable) ?? null)
      })
      .catch(() => {
        if (!canceled) {
          setActiveProvider(null)
        }
      })

    return () => {
      canceled = true
    }
  }, [])

  return (
    <aside aria-label={t.aiCopilot} className="ai-copilot-panel">
      <header className="ai-copilot-header">
        <div className="ai-copilot-title">
          <span aria-hidden="true" className="material-symbols-outlined ai-copilot-title-icon">
            auto_awesome
          </span>
          <span>
            <strong>{t.aiCopilot}</strong>
            <small>{t.aiCopilotPreview}</small>
          </span>
        </div>
        <div className="ai-copilot-header-actions">
          <button
            aria-label={t.aiCopilotConfigureProvider}
            className="ai-copilot-icon-button"
            title={t.aiCopilotConfigureProvider}
            type="button"
            onClick={onOpenSettings}
          >
            <span aria-hidden="true" className="material-symbols-outlined">
              settings_suggest
            </span>
          </button>
          <CloseButton aria-label={t.closeAiCopilot} onClick={onClose} size="compact" />
        </div>
      </header>

      <section className="ai-copilot-context-strip" aria-label={t.aiCopilotCurrentTerminal}>
        <span aria-hidden="true" className={`ai-copilot-target-icon ${isSshTarget ? 'is-connected' : ''}`}>
          <span className="material-symbols-outlined">terminal</span>
        </span>
        <span className="ai-copilot-target-copy">
          <strong>{targetLabel}</strong>
          <small>{isSshTarget ? workingDirectory : t.aiCopilotNoTerminalDescription}</small>
        </span>
        <span className="ai-copilot-context-status">{t.aiCopilotContextOff}</span>
      </section>

      <div className="ai-copilot-content">
        <section className="ai-copilot-setup" aria-labelledby="ai-copilot-setup-title">
          <span aria-hidden="true" className="material-symbols-outlined ai-copilot-setup-orb">
            auto_awesome
          </span>
          <span className="ai-copilot-eyebrow">{t.aiCopilotPreview}</span>
          <h2 id="ai-copilot-setup-title">
            {activeProvider ? t.aiCopilotProviderConfigured : t.aiCopilotNotConfigured}
          </h2>
          <p>
            {activeProvider
              ? activeProvider.name + ' · ' + activeProvider.model + ' · ' + t.aiCopilotProviderConfiguredDescription
              : t.aiCopilotNotConfiguredDescription}
          </p>
          <button className="ai-copilot-setup-action" type="button" onClick={onOpenSettings}>
            <span aria-hidden="true" className="material-symbols-outlined">
              tune
            </span>
            <span>{t.aiCopilotConfigureProvider}</span>
            <span aria-hidden="true" className="material-symbols-outlined ai-copilot-action-arrow">
              arrow_forward
            </span>
          </button>
        </section>

        <section className="ai-copilot-principles" aria-label={t.aiCopilotPreview}>
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
        </section>
      </div>

      <footer className="ai-copilot-composer-zone">
        <div className="ai-copilot-composer is-disabled">
          <textarea
            aria-label={t.aiCopilotInputAria}
            disabled
            placeholder={activeProvider ? t.aiCopilotChatComingSoon : t.aiCopilotComposerLocked}
            rows={3}
          />
          <div className="ai-copilot-composer-toolbar">
            <span>
              <span aria-hidden="true" className="material-symbols-outlined">
                lock
              </span>
              {t.aiCopilotNoTerminalOutput}
            </span>
            <button aria-label={t.aiCopilotSend} disabled type="button">
              <span aria-hidden="true" className="material-symbols-outlined">
                arrow_upward
              </span>
            </button>
          </div>
        </div>
      </footer>
    </aside>
  )
}
