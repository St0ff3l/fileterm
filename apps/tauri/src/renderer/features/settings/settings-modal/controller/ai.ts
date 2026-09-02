import { useEffect, useMemo } from 'react'
import type { AiModelCapabilities, AiProviderDraft, AiProviderSummary } from '@fileterm/core'
import { t } from '../../../../i18n'
import { waitForMinimumBusyDuration } from '../../../common/operation-timing'
import {
  AI_PROVIDER_PRESETS,
  DEFAULT_MODELS_BY_KIND,
  aiProviderRequestUrlPreview,
  aiProviderToDraft,
  createAiProviderDraft,
  isAiManualCapabilityProvider,
  type AiProviderPreset
} from '../constants'
import type { SettingsModalDesktopApi, SettingsModalState } from './state'

// Settings can be rendered inline or through the modal portal. Share the
// action lock across both instances so a transition between those surfaces
// cannot submit the same provider operation twice.
let aiProviderActionInFlight = false

export function useAiSettingsController({
  state,
  desktopApi
}: {
  state: SettingsModalState
  desktopApi: SettingsModalDesktopApi
}) {
  const {
    activeTab,
    aiProviders,
    setAiProviders,
    aiDraft,
    setAiDraft,
    aiModelChoices,
    setAiModelChoices,
    configuredModels,
    setConfiguredModels,
    selectedCandidateModel,
    setSelectedCandidateModel,
    isCustomInput,
    setIsCustomInput,
    customModelText,
    setCustomModelText,
    aiApiKey,
    setAiApiKey,
    clearAiApiKey,
    setClearAiApiKey,
    aiMessage,
    setAiMessage,
    aiOperation,
    setAiOperation,
    aiActionInFlightRef,
    showDeleteAiProviderConfirm,
    setShowDeleteAiProviderConfirm
  } = state

  const patchAiDraft = (patch: Partial<AiProviderDraft>) => {
    setAiDraft((current) => ({ ...current, ...patch }))
  }

  const patchAiModelCapabilities = (modelName: string, capabilities: AiModelCapabilities) => {
    setAiDraft((current) => ({
      ...current,
      modelCapabilities: {
        ...(current.modelCapabilities ?? {}),
        [modelName]: capabilities
      }
    }))
  }

  const selectAiProvider = (provider: AiProviderSummary | undefined) => {
    const draft = provider ? aiProviderToDraft(provider) : createAiProviderDraft(aiProviders.length === 0)
    if (!provider) {
      draft.model = ''
    }
    const providerModels =
      provider?.models && provider.models.length > 0 ? provider.models : provider?.model ? [provider.model] : []
    if (!isAiManualCapabilityProvider(draft)) {
      draft.modelCapabilities = {}
    }
    setAiDraft(draft)
    const presetMatch = AI_PROVIDER_PRESETS.find(
      (preset) => preset.draft.baseUrl === draft.baseUrl || preset.draft.name.toLowerCase() === draft.name.toLowerCase()
    )
    const defaultModels = presetMatch ? (presetMatch.draft.models ?? []) : (DEFAULT_MODELS_BY_KIND[draft.kind] ?? [])
    // Saved model IDs always stay visible, including for custom Providers that
    // do not match an internal preset. Preset/kind models are suggestions only.
    setAiModelChoices([...new Set([...providerModels, draft.model, ...defaultModels].filter(Boolean))])
    setConfiguredModels(providerModels)
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
    setAiApiKey('')
    setClearAiApiKey(false)
    setAiMessage(null)
  }

  // Apply a curated preset without wiping the draft's identity (`id`) or the
  // user's `enabled` / `isDefault` choices. The API key stays untouched.
  const applyAiPreset = (preset: AiProviderPreset) => {
    setAiDraft((current) => ({
      ...current,
      name: preset.draft.name,
      kind: preset.draft.kind,
      baseUrl: preset.draft.baseUrl,
      model: '',
      modelCapabilities: {},
      allowNoAuth: preset.draft.allowNoAuth,
      allowInsecureHttp: preset.draft.allowInsecureHttp
    }))
    const presetModels = preset.draft.models ?? (preset.draft.model ? [preset.draft.model] : [])
    setAiModelChoices(presetModels)
    setConfiguredModels([])
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
    setAiMessage(null)
  }

  const addSelectedModelToProvider = () => {
    const modelToAdd = (isCustomInput ? customModelText : selectedCandidateModel).trim()
    if (!modelToAdd) return

    setConfiguredModels((previous) => [...new Set([...previous, modelToAdd])])
    setAiModelChoices((previous) => [...new Set([modelToAdd, ...previous])])
    patchAiDraft({ model: modelToAdd })
    setSelectedCandidateModel('')
    setIsCustomInput(false)
    setCustomModelText('')
  }

  const removeConfiguredModel = (modelName: string) => {
    setConfiguredModels((previous) => previous.filter((model) => model !== modelName))
    setAiDraft((current) => {
      const nextModels = configuredModels.filter((model) => model !== modelName)
      const modelCapabilities = { ...(current.modelCapabilities ?? {}) }
      delete modelCapabilities[modelName]
      return {
        ...current,
        model: current.model === modelName ? (nextModels[0] ?? '') : current.model,
        modelCapabilities
      }
    })
  }

  const candidateModelOptions = useMemo(
    () => [...new Set([...aiModelChoices, ...configuredModels].filter(Boolean))],
    [aiModelChoices, configuredModels]
  )

  const aiProviderInput = () => {
    const secrets = clearAiApiKey ? { apiKey: null } : aiApiKey.trim() ? { apiKey: aiApiKey } : undefined
    const activeModel = aiDraft.model || configuredModels[0] || ''
    const modelCapabilities = isAiManualCapabilityProvider(aiDraft)
      ? Object.fromEntries(
          Object.entries(aiDraft.modelCapabilities ?? {}).filter(([model]) => configuredModels.includes(model))
        )
      : {}
    return {
      provider: {
        ...aiDraft,
        name: aiDraft.name.trim(),
        model: activeModel,
        models: configuredModels,
        modelCapabilities
      },
      ...(secrets ? { secrets } : {})
    }
  }

  const refreshAiModelChoices = async () => {
    if (!desktopApi || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return

    const operationStartedAt = performance.now()
    setAiOperation('models')
    try {
      const models = await desktopApi.listAiModels(aiProviderInput())
      const modelIds = models.map((model) => model.id).filter(Boolean)
      setAiModelChoices((previous) => [...new Set([...modelIds, ...previous])])
      setAiMessage({
        kind: 'success',
        message:
          modelIds.length > 0 ? `${t.aiSettingsModelListUpdated} (${modelIds.length})` : t.aiSettingsModelListEmpty
      })
    } catch (error) {
      setAiMessage({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      setAiOperation(null)
    }
  }

  const saveAiProvider = async () => {
    if (!desktopApi || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return
    const trimmedName = aiDraft.name.trim()
    if (!trimmedName) {
      setAiMessage({ kind: 'error', message: 'Provider 名称不能为空' })
      return
    }
    const duplicate = aiProviders.find(
      (provider) => provider.name.trim().toLowerCase() === trimmedName.toLowerCase() && provider.id !== aiDraft.id
    )
    if (duplicate) {
      setAiMessage({ kind: 'error', message: `Provider 名称 "${trimmedName}" 已存在，请使用其他唯一名称` })
      return
    }
    if (configuredModels.length === 0) {
      setAiMessage({ kind: 'error', message: '请至少添加一个模型到 Provider' })
      return
    }

    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    // Keep the current footer message while the request is in flight. Clearing
    // it would briefly render the idle test hint for fast save requests.
    const operationStartedAt = performance.now()
    setAiOperation('save')
    try {
      const saved = await desktopApi.saveAiProvider(aiProviderInput())
      const providers = await desktopApi.listAiProviders()
      setAiProviders(providers)
      const selected = providers.find((provider) => provider.id === saved.id) ?? saved
      setAiDraft(aiProviderToDraft(selected))
      const savedModels =
        selected.models && selected.models.length > 0 ? selected.models : selected.model ? [selected.model] : []
      setConfiguredModels(savedModels)
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage({ kind: 'success', message: t.aiSettingsSaveSucceeded })
    } catch (error) {
      setAiMessage({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiApiKey('')
      setClearAiApiKey(false)
      setAiOperation(null)
    }
  }

  const testAiProvider = async () => {
    if (!desktopApi || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return
    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    const operationStartedAt = performance.now()
    setAiOperation('test')
    try {
      const result = await desktopApi.testAiProvider(aiProviderInput())
      setAiMessage({ kind: 'success', message: result.message })
    } catch (error) {
      setAiMessage({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiOperation(null)
    }
  }

  const deleteAiProvider = async () => {
    if (!desktopApi || !aiDraft.id || aiOperation || aiActionInFlightRef.current || aiProviderActionInFlight) return

    aiActionInFlightRef.current = true
    aiProviderActionInFlight = true
    const operationStartedAt = performance.now()
    setAiOperation('delete')
    try {
      const providers = await desktopApi.deleteAiProvider(aiDraft.id)
      setAiProviders(providers)
      const fallback = providers.find((provider) => provider.isDefault) ?? providers[0]
      selectAiProvider(fallback)
      setAiApiKey('')
      setClearAiApiKey(false)
      window.dispatchEvent(new Event('fileterm:ai-providers-changed'))
      setAiMessage({ kind: 'success', message: t.aiSettingsDeleteSucceeded })
      setShowDeleteAiProviderConfirm(false)
    } catch (error) {
      setAiMessage({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      aiActionInFlightRef.current = false
      aiProviderActionInFlight = false
      setAiOperation(null)
    }
  }

  useEffect(() => {
    if (activeTab !== 'ai') return
    if (!desktopApi) {
      setAiMessage({ kind: 'error', message: t.aiSettingsDesktopOnly })
      return
    }

    let canceled = false
    setAiOperation('load')
    setAiMessage(null)
    void desktopApi
      .listAiProviders()
      .then((providers) => {
        if (canceled) return
        setAiProviders(providers)
        const current = aiDraft.id ? providers.find((provider) => provider.id === aiDraft.id) : undefined
        const nextProvider = current ?? providers.find((provider) => provider.isDefault) ?? providers[0]
        selectAiProvider(nextProvider)
        setAiApiKey('')
      })
      .catch((error: unknown) => {
        if (!canceled) {
          setAiMessage({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
        }
      })
      .finally(() => {
        if (!canceled) {
          setAiOperation(null)
        }
      })

    return () => {
      canceled = true
    }
    // selectAiProvider is intentionally omitted: this effect runs once per tab activation.
  }, [activeTab, desktopApi])

  const selectedAiProvider = aiDraft.id ? aiProviders.find((provider) => provider.id === aiDraft.id) : undefined
  const aiRequestUrlPreview = aiProviderRequestUrlPreview(aiDraft)

  return {
    selectedAiProvider,
    aiProviders,
    aiDraft,
    candidateModelOptions,
    configuredModels,
    selectedCandidateModel,
    setSelectedCandidateModel,
    isCustomInput,
    setIsCustomInput,
    customModelText,
    setCustomModelText,
    aiApiKey,
    setAiApiKey,
    clearAiApiKey,
    setClearAiApiKey,
    aiMessage,
    aiOperation,
    aiRequestUrlPreview,
    applyAiPreset,
    selectAiProvider,
    patchAiDraft,
    patchAiModelCapabilities,
    refreshAiModelChoices,
    addSelectedModelToProvider,
    removeConfiguredModel,
    saveAiProvider,
    testAiProvider,
    showDeleteAiProviderConfirm,
    setShowDeleteAiProviderConfirm,
    deleteAiProvider
  }
}
