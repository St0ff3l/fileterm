import type {
  AiModelCapabilities,
  AiModelReasoningConfig,
  AiModelReasoningMode,
  AiModelReasoningParameter,
  AiProviderDraft,
  AiReasoningEffort
} from '@fileterm/core'
import { AppIcon } from '../../../common/app-icon'
import { DropdownSelect } from '../../../common/dropdown-select'
import { SelectionControl } from '../../../common/selection-control'
import { getAiModelCapabilities } from '../../../ai/ai-reasoning'
import type { LocaleMessages } from '../../../../i18n'

type ExplicitEffort = Exclude<AiReasoningEffort, 'auto'>
type BudgetEffort = Exclude<ExplicitEffort, 'none'>

const REASONING_EFFORTS = [
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max'
] as const satisfies readonly ExplicitEffort[]
const BUDGET_EFFORTS = ['minimal', 'low', 'medium', 'high', 'xhigh', 'max'] as const satisfies readonly BudgetEffort[]

const REASONING_PARAMETER_LABELS: Record<AiModelReasoningParameter, string> = {
  auto: 'Auto',
  'reasoning-effort': 'reasoning_effort',
  'reasoning-object': 'reasoning: { effort }',
  'output-config-effort': 'output_config.effort',
  'thinking-toggle': 'thinking.type',
  'thinking-budget': 'enable_thinking + thinking_budget',
  'chat-template-reasoning-effort': 'chat_template_kwargs.reasoning_effort'
}

function defaultParameterForMode(mode: AiModelReasoningMode): AiModelReasoningParameter {
  switch (mode) {
    case 'budget':
      return 'thinking-budget'
    case 'toggle':
      return 'thinking-toggle'
    case 'effort':
      return 'reasoning-effort'
    case 'none':
      return 'auto'
  }
}

export function AiModelCapabilityEditor({
  t,
  provider,
  modelName,
  capabilities: declaredCapabilities,
  onChange
}: {
  t: LocaleMessages
  provider: AiProviderDraft
  modelName: string
  capabilities?: AiModelCapabilities
  onChange(capabilities: AiModelCapabilities): void
}) {
  const capabilities = declaredCapabilities ?? getAiModelCapabilities(provider, modelName)
  const reasoning = capabilities.reasoning
  const reasoningModeOptions = [
    { value: 'none', label: t.aiSettingsModelReasoningNone },
    { value: 'effort', label: t.aiSettingsModelReasoningEffort },
    { value: 'budget', label: t.aiSettingsModelReasoningBudget },
    { value: 'toggle', label: t.aiSettingsModelReasoningToggle }
  ]
  const reasoningParameterOptions = Object.entries(REASONING_PARAMETER_LABELS).map(([value, label]) => ({
    value,
    label
  }))

  const updateReasoning = (patch: Partial<AiModelReasoningConfig>) => {
    onChange({
      ...capabilities,
      reasoning: {
        ...reasoning,
        ...patch
      }
    })
  }

  const toggleEffort = (effort: ExplicitEffort) => {
    const efforts = reasoning.efforts.includes(effort)
      ? reasoning.efforts.filter((value) => value !== effort)
      : [...reasoning.efforts, effort]
    updateReasoning({ efforts })
  }

  const updateBudget = (effort: BudgetEffort, rawValue: string) => {
    const budgets = { ...(reasoning.budgets ?? {}) }
    if (!rawValue.trim()) {
      delete budgets[effort]
    } else {
      const value = Number(rawValue)
      if (!Number.isFinite(value)) return
      budgets[effort] = Math.min(1_000_000, Math.max(1, Math.floor(value)))
    }
    updateReasoning({ budgets })
  }

  const imageInputEnabled = capabilities.inputModalities.includes('image')

  return (
    <section className="ai-model-capability-editor" aria-labelledby="ai-model-capability-editor-title">
      <div className="ai-model-capability-header">
        <div className="ai-model-capability-title-group">
          <strong id="ai-model-capability-editor-title">{t.aiSettingsModelCapabilitiesTitle}</strong>
          <code>{modelName}</code>
        </div>
        <span className="ai-model-capability-manual-tag">
          <AppIcon name="edit" size={13} />
          {t.aiSettingsModelCapabilitiesManual}
        </span>
      </div>
      <p className="ai-model-capability-description">{t.aiSettingsModelCapabilitiesHint}</p>

      <div className="ai-model-capability-grid">
        <label className="ai-model-capability-field">
          <span>{t.aiSettingsModelReasoning}</span>
          <DropdownSelect
            ariaLabel={t.aiSettingsModelReasoning}
            value={reasoning.mode}
            options={reasoningModeOptions}
            onChange={(value) => {
              const mode = value as AiModelReasoningMode
              updateReasoning({
                mode,
                parameter: defaultParameterForMode(mode),
                efforts: mode === 'none' ? [] : reasoning.efforts.length > 0 ? reasoning.efforts : ['low']
              })
            }}
          />
        </label>
        <label className="ai-model-capability-field">
          <span>{t.aiSettingsModelReasoningParameter}</span>
          <DropdownSelect
            ariaLabel={t.aiSettingsModelReasoningParameter}
            disabled={reasoning.mode === 'none'}
            value={reasoning.parameter}
            options={reasoningParameterOptions}
            onChange={(value) => updateReasoning({ parameter: value as AiModelReasoningParameter })}
          />
        </label>
      </div>

      {reasoning.mode !== 'none' ? (
        <div className="ai-model-capability-field ai-model-capability-effort-field">
          <span>{t.aiSettingsModelReasoningValues}</span>
          <small>{t.aiSettingsModelReasoningValuesHint}</small>
          <div className="ai-model-capability-effort-list" role="group" aria-label={t.aiSettingsModelReasoningValues}>
            {REASONING_EFFORTS.map((effort) => {
              const selected = reasoning.efforts.includes(effort)
              return (
                <button
                  key={effort}
                  aria-pressed={selected}
                  className={`ai-model-capability-effort-chip ${selected ? 'is-selected' : ''}`}
                  type="button"
                  onClick={() => toggleEffort(effort)}
                >
                  {effort}
                </button>
              )
            })}
          </div>
        </div>
      ) : null}

      {reasoning.mode === 'budget' && (
        <div className="ai-model-capability-field ai-model-capability-budget-field">
          <span>{t.aiSettingsModelReasoningBudgets}</span>
          <small>{t.aiSettingsModelReasoningBudgetsHint}</small>
          <div className="ai-model-capability-budget-list">
            {BUDGET_EFFORTS.filter((effort) => reasoning.efforts.includes(effort)).map((effort) => (
              <label key={effort}>
                <span>{effort}</span>
                <input
                  aria-label={`${effort} ${t.aiSettingsModelReasoningBudgets}`}
                  inputMode="numeric"
                  max={1_000_000}
                  min={1}
                  type="number"
                  value={reasoning.budgets?.[effort] ?? ''}
                  onChange={(event) => updateBudget(effort, event.target.value)}
                />
              </label>
            ))}
          </div>
        </div>
      )}

      <label className="ai-model-capability-image-row ssh-checkbox">
        <SelectionControl
          checked={imageInputEnabled}
          type="checkbox"
          onChange={(event) => {
            const inputModalities: AiModelCapabilities['inputModalities'] = capabilities.inputModalities.filter(
              (modality) => modality !== 'image'
            )
            if (event.target.checked) inputModalities.push('image')
            onChange({ ...capabilities, inputModalities })
          }}
        />
        <span>
          <strong>{t.aiSettingsModelImageInput}</strong>
          <small>{t.aiSettingsModelImageInputHint}</small>
        </span>
      </label>
    </section>
  )
}
