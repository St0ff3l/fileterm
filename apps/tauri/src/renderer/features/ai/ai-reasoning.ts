import type { AiModelCapabilities, AiModelReasoningMode, AiProviderSummary, AiReasoningEffort } from '@fileterm/core'
import { isAiManualCapabilityProvider } from '../settings/settings-modal/constants'

export type AiReasoningControl = 'effort' | 'budget' | 'toggle' | 'none'

export interface AiReasoningProfile {
  /** The wire-level control used by the provider for this model. */
  control: AiReasoningControl
  /** Explicit values supported by the model; `auto` is an internal fallback. */
  efforts: readonly Exclude<AiReasoningEffort, 'auto'>[]
}

type AiProviderModelIdentity = Pick<AiProviderSummary, 'kind' | 'baseUrl' | 'name' | 'modelCapabilities'>

const WIDELY_SUPPORTED_EFFORTS = ['low', 'medium', 'high'] as const satisfies readonly AiReasoningEffort[]
const OPENAI_STANDARD_EFFORTS = [
  'none',
  'low',
  'medium',
  'high',
  'xhigh'
] as const satisfies readonly AiReasoningEffort[]
const OPENAI_GPT_56_EFFORTS = [
  'none',
  'low',
  'medium',
  'high',
  'xhigh',
  'max'
] as const satisfies readonly AiReasoningEffort[]
const OPENAI_PRO_EFFORTS = ['medium', 'high', 'xhigh'] as const satisfies readonly AiReasoningEffort[]
const ANTHROPIC_ADAPTIVE_EFFORTS = [
  'low',
  'medium',
  'high',
  'xhigh',
  'max'
] as const satisfies readonly AiReasoningEffort[]
const GLM_52_EFFORTS = [
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max'
] as const satisfies readonly AiReasoningEffort[]
const DEEPSEEK_EFFORTS = ['none', 'low', 'high', 'max'] as const satisfies readonly AiReasoningEffort[]
const KIMI_K3_EFFORTS = ['low', 'high', 'max'] as const satisfies readonly AiReasoningEffort[]
const VOLCENGINE_EFFORTS = ['minimal', 'low', 'medium', 'high'] as const satisfies readonly AiReasoningEffort[]
const BUDGET_EFFORTS = ['low', 'medium', 'high', 'max'] as const satisfies readonly AiReasoningEffort[]

const AUTO_ONLY_PROFILE: AiReasoningProfile = { control: 'none', efforts: [] }
const TOGGLE_ONLY_PROFILE: AiReasoningProfile = { control: 'toggle', efforts: [] }
const OPENAI_STANDARD_PROFILE: AiReasoningProfile = { control: 'effort', efforts: OPENAI_STANDARD_EFFORTS }
const OPENAI_GPT_56_PROFILE: AiReasoningProfile = { control: 'effort', efforts: OPENAI_GPT_56_EFFORTS }
const OPENAI_PRO_PROFILE: AiReasoningProfile = { control: 'effort', efforts: OPENAI_PRO_EFFORTS }
const ANTHROPIC_ADAPTIVE_PROFILE: AiReasoningProfile = {
  control: 'effort',
  efforts: ANTHROPIC_ADAPTIVE_EFFORTS
}
const GLM_52_PROFILE: AiReasoningProfile = { control: 'effort', efforts: GLM_52_EFFORTS }
const DEEPSEEK_PROFILE: AiReasoningProfile = {
  control: 'effort',
  efforts: DEEPSEEK_EFFORTS
}
const KIMI_K3_PROFILE: AiReasoningProfile = { control: 'effort', efforts: KIMI_K3_EFFORTS }
const VOLCENGINE_PROFILE: AiReasoningProfile = { control: 'effort', efforts: VOLCENGINE_EFFORTS }
const BUDGET_PROFILE: AiReasoningProfile = { control: 'budget', efforts: BUDGET_EFFORTS }

function normalize(value: string) {
  return value.trim().toLocaleLowerCase()
}

function modelMatches(model: string, ...candidates: string[]) {
  return candidates.some((candidate) => model === candidate || model.endsWith(`/${candidate}`))
}

function modelStartsWith(model: string, prefix: string) {
  return model === prefix || model.startsWith(`${prefix}-`) || model.startsWith(`${prefix}/`)
}

function providerIdentity(provider: AiProviderModelIdentity) {
  return `${normalize(provider.baseUrl)} ${normalize(provider.name)}`
}

function explicitModelCapabilities(provider: AiProviderModelIdentity, model: string) {
  const capabilities = provider.modelCapabilities
  if (!capabilities) return undefined
  return (
    capabilities[model] ??
    capabilities[Object.keys(capabilities).find((key) => normalize(key) === normalize(model)) ?? '']
  )
}

function explicitReasoningProfile(provider: AiProviderModelIdentity, model: string): AiReasoningProfile | undefined {
  const reasoning = explicitModelCapabilities(provider, model)?.reasoning
  if (!reasoning) return undefined
  const efforts = reasoning.efforts
  switch (reasoning.mode) {
    case 'effort':
      return { control: 'effort', efforts }
    case 'budget':
      return { control: 'budget', efforts }
    case 'toggle':
      return { control: 'toggle', efforts: efforts.length > 0 ? efforts : ['none', 'low'] }
    case 'none':
      return AUTO_ONLY_PROFILE
  }
}

function openAiProfile(model: string): AiReasoningProfile {
  if (modelMatches(model, 'gpt-5.6-pro', 'gpt-5.5-pro', 'gpt-5.4-pro')) return OPENAI_PRO_PROFILE
  if (modelStartsWith(model, 'gpt-5.6')) return OPENAI_GPT_56_PROFILE
  if (
    modelStartsWith(model, 'gpt-5.5') ||
    modelStartsWith(model, 'gpt-5.4') ||
    modelStartsWith(model, 'gpt-5.3-codex') ||
    modelStartsWith(model, 'gpt-5.2')
  ) {
    return OPENAI_STANDARD_PROFILE
  }
  if (modelMatches(model, 'o3')) return { control: 'effort', efforts: WIDELY_SUPPORTED_EFFORTS }
  return AUTO_ONLY_PROFILE
}

function anthropicProfile(model: string): AiReasoningProfile {
  if (
    modelStartsWith(model, 'claude-fable-5') ||
    modelStartsWith(model, 'claude-opus-5') ||
    modelStartsWith(model, 'claude-sonnet-5') ||
    modelStartsWith(model, 'claude-opus-4-8') ||
    modelStartsWith(model, 'claude-opus-4-7') ||
    modelStartsWith(model, 'claude-opus-4-6') ||
    modelStartsWith(model, 'claude-sonnet-4-6')
  ) {
    return ANTHROPIC_ADAPTIVE_PROFILE
  }
  // Claude 4.5 and Haiku 4.5 use the older token-budget API. Keep them on
  // Auto until a budget-specific request adapter is added.
  if (modelStartsWith(model, 'claude-haiku-4-5')) return AUTO_ONLY_PROFILE
  return AUTO_ONLY_PROFILE
}

function glmProfile(model: string): AiReasoningProfile {
  if (modelMatches(model, 'glm-5.2')) return GLM_52_PROFILE
  if (
    modelMatches(
      model,
      'glm-5.1',
      'glm-5-turbo',
      'glm-5',
      'glm-5v-turbo',
      'glm-4.7',
      'glm-4.6',
      'glm-4.6v',
      'glm-4.5',
      'glm-4.5v'
    )
  ) {
    return TOGGLE_ONLY_PROFILE
  }
  return AUTO_ONLY_PROFILE
}

function deepSeekProfile(model: string): AiReasoningProfile {
  if (modelStartsWith(model, 'deepseek-v4-flash') || modelStartsWith(model, 'deepseek-v4-pro')) {
    return DEEPSEEK_PROFILE
  }
  return AUTO_ONLY_PROFILE
}

function kimiProfile(model: string): AiReasoningProfile {
  if (modelMatches(model, 'kimi-k3')) return KIMI_K3_PROFILE
  if (modelMatches(model, 'kimi-k2.6')) return TOGGLE_ONLY_PROFILE
  // K2.7 Code is always thinking and does not expose a reasoning_effort
  // parameter. It therefore remains Auto-only in the selector.
  return AUTO_ONLY_PROFILE
}

function volcengineProfile(model: string): AiReasoningProfile {
  if (modelStartsWith(model, 'doubao-seed-2-0') || modelStartsWith(model, 'doubao-seed-2-1')) {
    return VOLCENGINE_PROFILE
  }
  return AUTO_ONLY_PROFILE
}

function siliconFlowProfile(model: string): AiReasoningProfile {
  if (
    modelMatches(
      model,
      'deepseek-ai/deepseek-v3.2',
      'deepseek-ai/deepseek-v3.1-terminus',
      'deepseek-ai/deepseek-r1',
      'moonshotai/kimi-k2.6',
      'zai-org/glm-5.1',
      'zai-org/glm-5',
      'zai-org/glm-4.7',
      'minimaxai/minimax-m2.5'
    ) ||
    modelStartsWith(model, 'qwen/qwen3')
  ) {
    return BUDGET_PROFILE
  }
  return AUTO_ONLY_PROFILE
}

function compatibleProfile(model: string): AiReasoningProfile {
  return glmProfile(model).efforts.length > 0
    ? glmProfile(model)
    : deepSeekProfile(model).efforts.length > 0
      ? deepSeekProfile(model)
      : kimiProfile(model).efforts.length > 0
        ? kimiProfile(model)
        : openAiProfile(model)
}

/**
 * Mirrors OpenCode/DBX's model capability gate: known models get the exact
 * effort values their provider advertises; manual sources can add explicit
 * capabilities for models that are not recognized by the built-in adapters.
 */
export function getAiReasoningProfile(provider: AiProviderModelIdentity, model: string): AiReasoningProfile {
  const normalizedModel = normalize(model)
  if (!normalizedModel) return AUTO_ONLY_PROFILE

  const explicit = explicitReasoningProfile(provider, model)
  if (explicit) return explicit

  if (provider.kind === 'anthropic-messages') return anthropicProfile(normalizedModel)
  if (provider.kind === 'openai-responses') return openAiProfile(normalizedModel)

  const identity = providerIdentity(provider)
  if (identity.includes('siliconflow')) return siliconFlowProfile(normalizedModel)
  if (identity.includes('volces') || identity.includes('火山') || identity.includes('volcengine')) {
    return volcengineProfile(normalizedModel)
  }
  if (identity.includes('bigmodel') || identity.includes('智谱') || identity.includes('zhipu')) {
    return glmProfile(normalizedModel)
  }
  if (identity.includes('moonshot') || identity.includes('kimi')) return kimiProfile(normalizedModel)
  if (identity.includes('deepseek')) return deepSeekProfile(normalizedModel)
  return compatibleProfile(normalizedModel)
}

function knownImageInput(provider: AiProviderModelIdentity, model: string) {
  const normalizedModel = normalize(model)
  if (
    normalizedModel.includes('vision') ||
    normalizedModel.includes('-vl') ||
    normalizedModel.endsWith('v') ||
    normalizedModel.includes('ocr')
  ) {
    return true
  }
  if (provider.kind === 'openai-responses' && modelStartsWith(normalizedModel, 'gpt-5')) {
    return true
  }
  if (provider.kind === 'anthropic-messages' && modelStartsWith(normalizedModel, 'claude-')) {
    return true
  }
  if (
    (provider.kind === 'openai-compatible-chat' &&
      (modelStartsWith(normalizedModel, 'kimi-k3') ||
        modelStartsWith(normalizedModel, 'kimi-k2.7') ||
        modelStartsWith(normalizedModel, 'kimi-k2.6') ||
        modelStartsWith(normalizedModel, 'doubao-seed-2-'))) ||
    modelMatches(normalizedModel, 'moonshotai/kimi-k2.6')
  ) {
    return true
  }
  return false
}

export function getAiModelCapabilities(provider: AiProviderModelIdentity, model: string): AiModelCapabilities {
  const explicit = explicitModelCapabilities(provider, model)
  if (explicit) return explicit

  const profile = getAiReasoningProfile(provider, model)
  const mode: AiModelReasoningMode = profile.control
  const efforts =
    profile.control === 'toggle' && profile.efforts.length === 0 ? (['none', 'low'] as const) : profile.efforts
  return {
    inputModalities: knownImageInput(provider, model) ? ['text', 'image'] : ['text'],
    reasoning: {
      mode,
      parameter: 'auto',
      efforts: [...efforts]
    }
  }
}

export function getAiReasoningOptions(provider: AiProviderModelIdentity, model: string): AiReasoningEffort[] {
  const profile = getAiReasoningProfile(provider, model)
  const efforts: AiReasoningEffort[] =
    profile.control === 'toggle' && profile.efforts.length === 0 ? ['none', 'low'] : [...profile.efforts]
  // `auto` is useful for manually configured/local sources because the user
  // may not know which wire-level reasoning control their endpoint accepts.
  // Template providers expose only the explicit values documented for them.
  return isAiManualCapabilityProvider(provider) ? ['auto', ...efforts] : efforts
}

export function supportsAiReasoningEffort(provider: AiProviderModelIdentity, model: string, effort: AiReasoningEffort) {
  return getAiReasoningOptions(provider, model).includes(effort)
}

export function supportsAiModelInput(
  provider: AiProviderModelIdentity,
  model: string,
  modality: AiModelCapabilities['inputModalities'][number]
) {
  return getAiModelCapabilities(provider, model).inputModalities.includes(modality)
}
