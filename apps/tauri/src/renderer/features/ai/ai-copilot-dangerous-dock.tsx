import type { AiCopilotModeState } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StableButtonLabel } from '../common/stable-button-content'

export function AiCopilotDangerousDock({
  dangerousCommandRestrictionsEnabled,
  isStreaming,
  modeState,
  showsDangerousCommandRestrictions,
  toggleDangerousCommandRestrictions
}: {
  dangerousCommandRestrictionsEnabled: boolean
  isStreaming: boolean
  modeState: AiCopilotModeState | null
  showsDangerousCommandRestrictions: boolean
  toggleDangerousCommandRestrictions(): void
}) {
  return (
    <section
      aria-label={showsDangerousCommandRestrictions ? t.aiCopilotDangerousCommandRestrictions : undefined}
      className="ai-copilot-dangerous-command-dock"
    >
      {showsDangerousCommandRestrictions ? (
        <>
          <span className="ai-copilot-dangerous-command-dock-hint">{t.aiCopilotDangerousCommandRestrictionsHint}</span>
          <button
            aria-checked={dangerousCommandRestrictionsEnabled}
            aria-label={`${t.aiCopilotDangerousCommandRestrictions} · ${
              dangerousCommandRestrictionsEnabled
                ? t.aiCopilotDangerousCommandRestrictionsOn
                : t.aiCopilotDangerousCommandRestrictionsOff
            }`}
            className={`ai-copilot-dangerous-command-toggle ${
              dangerousCommandRestrictionsEnabled ? 'is-enabled' : 'is-disabled'
            }`}
            disabled={isStreaming || !modeState}
            role="switch"
            title={t.aiCopilotDangerousCommandRestrictionsDescription}
            type="button"
            onClick={toggleDangerousCommandRestrictions}
          >
            <AppIcon name={dangerousCommandRestrictionsEnabled ? 'shield-check' : 'shield'} size={12} />
            <span>{t.aiCopilotDangerousCommandRestrictions}</span>
            <StableButtonLabel
              label={
                <strong>
                  {dangerousCommandRestrictionsEnabled
                    ? t.aiCopilotDangerousCommandRestrictionsOn
                    : t.aiCopilotDangerousCommandRestrictionsOff}
                </strong>
              }
              reserveLabel={
                <strong>
                  {dangerousCommandRestrictionsEnabled
                    ? t.aiCopilotDangerousCommandRestrictionsOff
                    : t.aiCopilotDangerousCommandRestrictionsOn}
                </strong>
              }
            />
          </button>
        </>
      ) : null}
    </section>
  )
}
