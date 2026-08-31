import type { MutableRefObject } from 'react'
import type { AiCopilotMode, AiProviderSummary } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon, type AppIconName } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'

export function AiCopilotComposer({
  canChat,
  copilotMode,
  copilotModeDescription,
  copilotModeIconName,
  currentProvider,
  draft,
  isContextPreviewing,
  isStreaming,
  isTerminalTarget,
  onComposerKeyDown,
  providers,
  referenceTerminal,
  requiresTerminalContext,
  selectCopilotMode,
  selectModel,
  selectProvider,
  selectedModel,
  selectedProviderId,
  send,
  setDraft,
  stop,
  toggleTerminalReference,
  composerCompositionRef
}: {
  canChat: boolean
  copilotMode: AiCopilotMode
  copilotModeDescription(mode: AiCopilotMode): string
  copilotModeIconName(mode: AiCopilotMode): AppIconName
  currentProvider: AiProviderSummary | null
  draft: string
  isContextPreviewing: boolean
  isStreaming: boolean
  isTerminalTarget: boolean
  onComposerKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>): void
  providers: AiProviderSummary[]
  referenceTerminal: boolean
  requiresTerminalContext: boolean
  selectCopilotMode(nextMode: AiCopilotMode): void
  selectModel(model: string | null): void
  selectProvider(providerId: string | null): void
  selectedModel: string | null
  selectedProviderId: string | null
  send(): Promise<void>
  setDraft(value: string): void
  stop(): Promise<void>
  toggleTerminalReference(): void
  composerCompositionRef: MutableRefObject<boolean>
}) {
  return (
    <footer className="ai-copilot-composer-area">
      <div className="ai-copilot-composer-zone">
        {canChat ? (
          <section className="ai-copilot-context-dock" aria-label={t.aiCopilotReferenceTerminal}>
            <div className="ai-copilot-context-dock-row">
              <button
                aria-pressed={referenceTerminal}
                aria-label={t.aiCopilotReferenceTerminal}
                aria-busy={isContextPreviewing}
                className={`ai-copilot-context-switch ${referenceTerminal ? 'is-active' : ''}`}
                disabled={
                  isStreaming ||
                  isContextPreviewing ||
                  copilotMode !== 'pure-conversation' ||
                  (!isTerminalTarget && !referenceTerminal)
                }
                type="button"
                onClick={toggleTerminalReference}
              >
                <span aria-hidden="true" className="stable-button-icon ai-copilot-context-switch-icon">
                  {isContextPreviewing ? (
                    <span className="button-spinner" />
                  ) : (
                    <span className="material-symbols-outlined">
                      {referenceTerminal ? 'visibility' : 'visibility_off'}
                    </span>
                  )}
                </span>
                <span>{t.aiCopilotReferenceTerminal}</span>
                <span className="ai-copilot-context-switch-state">
                  {referenceTerminal ? t.aiCopilotReferenceTerminalOn : t.aiCopilotReferenceTerminalOff}
                </span>
              </button>
              <span className="ai-copilot-context-dock-hint">
                {copilotMode !== 'pure-conversation'
                  ? t.aiCopilotContextLockedByMode
                  : referenceTerminal
                    ? isTerminalTarget
                      ? t.aiCopilotContextAutoHint
                      : t.aiCopilotContextNeedsTerminal
                    : t.aiCopilotL0ComposerHint}
              </span>
            </div>
          </section>
        ) : null}
        <div className={`ai-copilot-composer ${!canChat ? 'is-disabled' : ''}`}>
          <textarea
            aria-label={t.aiCopilotInputAria}
            disabled={!canChat || isStreaming}
            placeholder={canChat ? t.aiCopilotPromptPlaceholder : t.aiCopilotComposerLocked}
            rows={3}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onCompositionEnd={() => {
              // Keep the guard active through the IME commit keydown.
              window.setTimeout(() => {
                composerCompositionRef.current = false
              }, 0)
            }}
            onCompositionStart={() => {
              composerCompositionRef.current = true
            }}
            onKeyDown={onComposerKeyDown}
          />
          <div className="ai-copilot-composer-toolbar">
            <div className="ai-copilot-composer-toolbar-actions">
              {canChat ? (
                <div className="ai-copilot-composer-models" aria-label={t.aiCopilotConversationControls}>
                  <DropdownSelect
                    className="ai-copilot-composer-select ai-copilot-provider-select"
                    disabled={isStreaming}
                    options={providers.map((provider) => ({ value: provider.id, label: provider.name }))}
                    value={selectedProviderId ?? ''}
                    onChange={(value) => selectProvider(value || null)}
                  />
                  {currentProvider ? (
                    <>
                      <span aria-hidden="true" className="ai-copilot-composer-model-divider" />
                      <DropdownSelect
                        className="ai-copilot-composer-select ai-copilot-model-select"
                        disabled={isStreaming}
                        options={(currentProvider.models && currentProvider.models.length > 0
                          ? currentProvider.models
                          : [currentProvider.model]
                        ).map((model) => ({ value: model, label: model }))}
                        value={selectedModel ?? currentProvider.model}
                        onChange={(value) => selectModel(value || null)}
                      />
                      <span aria-hidden="true" className="ai-copilot-composer-model-divider ai-copilot-mode-divider" />
                      <DropdownSelect
                        ariaLabel={t.aiCopilotModeLabel}
                        className="ai-copilot-composer-select ai-copilot-mode-select"
                        disabled={isStreaming}
                        forceCustomMenu
                        menuClassName="ai-copilot-mode-menu"
                        menuPlacement="above"
                        menuWidth="auto"
                        options={[
                          { value: 'pure-conversation', label: t.aiCopilotModePure },
                          { value: 'semi-automatic', label: t.aiCopilotModeSemi },
                          { value: 'fully-automatic', label: t.aiCopilotModeFull }
                        ]}
                        renderOption={(option, selected) => {
                          const optionMode = option.value as AiCopilotMode
                          return (
                            <span className="ai-copilot-mode-option">
                              <span className="ai-copilot-mode-option-copy">
                                <strong>
                                  <AppIcon
                                    className="ai-copilot-mode-option-icon"
                                    name={copilotModeIconName(optionMode)}
                                    size={12}
                                  />
                                  <span>{option.label}</span>
                                  {selected ? (
                                    <AppIcon className="ai-copilot-mode-option-check" name="check" size={13} />
                                  ) : null}
                                </strong>
                                <small>{copilotModeDescription(optionMode)}</small>
                              </span>
                            </span>
                          )
                        }}
                        renderValue={(option) => {
                          const optionMode = option.value as AiCopilotMode
                          return (
                            <span className="ai-copilot-mode-value">
                              <AppIcon
                                className="ai-copilot-mode-value-icon"
                                name={copilotModeIconName(optionMode)}
                                size={10}
                              />
                              <span>{option.label}</span>
                            </span>
                          )
                        }}
                        value={copilotMode}
                        onChange={(value) => selectCopilotMode(value as AiCopilotMode)}
                      />
                    </>
                  ) : null}
                </div>
              ) : null}
              {isStreaming ? (
                <button
                  aria-label={t.aiCopilotStop}
                  className="ai-copilot-composer-send is-stop"
                  type="button"
                  onClick={() => void stop()}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    stop
                  </span>
                </button>
              ) : (
                <button
                  aria-label={t.aiCopilotSend}
                  className="ai-copilot-composer-send"
                  disabled={
                    !canChat || !draft.trim() || (requiresTerminalContext && (!referenceTerminal || !isTerminalTarget))
                  }
                  type="button"
                  onClick={() => void send()}
                >
                  <AppIcon name="arrow-up" size={16} />
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </footer>
  )
}
