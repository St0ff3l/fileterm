import type { Dispatch, SetStateAction } from 'react'
import type { AiProviderDraft, AiProviderKind, AiProviderSummary, FileTermDesktopApi } from '@fileterm/core'
import { AppIcon } from '../../../common/app-icon'
import { ConfirmActionDialog } from '../../../common/confirm-action-dialog'
import { DropdownSelect } from '../../../common/dropdown-select'
import { SelectionControl } from '../../../common/selection-control'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import { AI_PROVIDER_PRESETS, type AiFeedback, type AiProviderPreset } from '../constants'

type AiSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  selectedAiProvider: AiProviderSummary | undefined
  aiProviders: AiProviderSummary[]
  aiDraft: AiProviderDraft
  candidateModelOptions: string[]
  configuredModels: string[]
  selectedCandidateModel: string
  setSelectedCandidateModel: Dispatch<SetStateAction<string>>
  isCustomInput: boolean
  setIsCustomInput: Dispatch<SetStateAction<boolean>>
  customModelText: string
  setCustomModelText: Dispatch<SetStateAction<string>>
  aiApiKey: string
  setAiApiKey: Dispatch<SetStateAction<string>>
  clearAiApiKey: boolean
  setClearAiApiKey: Dispatch<SetStateAction<boolean>>
  aiMessage: AiFeedback | null
  aiOperation: 'load' | 'save' | 'test' | 'delete' | null
  aiRequestUrlPreview: string | null
  applyAiPreset(preset: AiProviderPreset): void
  selectAiProvider(provider: AiProviderSummary | undefined): void
  patchAiDraft(patch: Partial<AiProviderDraft>): void
  addSelectedModelToProvider(): void
  removeConfiguredModel(modelName: string): void
  saveAiProvider(): Promise<void>
  testAiProvider(): Promise<void>
  showDeleteAiProviderConfirm: boolean
  setShowDeleteAiProviderConfirm: Dispatch<SetStateAction<boolean>>
  deleteAiProvider(): Promise<void>
}

export function AiSettingsPanel() {
  const {
    t,
    desktopApi,
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
    addSelectedModelToProvider,
    removeConfiguredModel,
    saveAiProvider,
    testAiProvider,
    showDeleteAiProviderConfirm,
    setShowDeleteAiProviderConfirm,
    deleteAiProvider
  } = useSettingsModalContext<AiSettingsPanelContext>()

  return (
    <div className="settings-panel settings-ai-panel">
      <section className="settings-section">
        <h3>{t.aiSettingsProvider}</h3>
        <p className="settings-tools-hint">{t.aiSettingsProviderDescription}</p>

        <div className="ai-settings-provider-card">
          <span aria-hidden="true" className="material-symbols-outlined">
            auto_awesome
          </span>
          <div>
            <strong>
              {selectedAiProvider?.usable
                ? t.aiSettingsProviderReady
                : selectedAiProvider
                  ? t.aiSettingsProviderNeedsAttention
                  : t.aiSettingsNotConfigured}
            </strong>
            <p>
              {aiRequestUrlPreview
                ? `${t.aiSettingsRequestUrlPreview} · ${aiRequestUrlPreview}`
                : selectedAiProvider
                  ? selectedAiProvider.baseUrl
                  : t.aiSettingsPreviewHint}
            </p>
          </div>
          <span className="ai-settings-preview-tag">
            {selectedAiProvider?.hasApiKey ? t.aiSettingsApiKeySaved : t.aiCopilotPreview}
          </span>
        </div>

        <div className="ai-settings-provider-picker">
          <label>
            <span>{t.aiSettingsConfiguredProviders}</span>
            <DropdownSelect
              disabled={!desktopApi || aiOperation !== null}
              value={aiDraft.id ?? '__new__'}
              options={[
                ...aiProviders.map((provider) => ({ value: provider.id, label: provider.name })),
                { value: '__new__', label: t.aiSettingsAddProvider }
              ]}
              onChange={(providerId) => {
                selectAiProvider(
                  providerId === '__new__' ? undefined : aiProviders.find((provider) => provider.id === providerId)
                )
              }}
            />
          </label>
          <button
            className="ai-settings-secondary-button"
            disabled={!desktopApi || aiOperation !== null}
            type="button"
            onClick={() => selectAiProvider(undefined)}
          >
            <AppIcon name="plus" size={14} />
            {t.aiSettingsAddProvider}
          </button>
        </div>

        {!aiDraft.id && (
          <div className="ai-settings-preset-picker">
            <span className="ai-settings-preset-label">{t.aiSettingsPresetLabel}</span>
            <DropdownSelect
              className="ai-settings-preset-select"
              disabled={!desktopApi || aiOperation !== null}
              value="__none__"
              placeholder={t.aiSettingsPresetPlaceholder}
              options={[
                { value: '__none__', label: t.aiSettingsPresetPlaceholder },
                ...AI_PROVIDER_PRESETS.map((preset) => ({
                  value: preset.id,
                  label: String(t[preset.labelKey])
                }))
              ]}
              onChange={(value) => {
                const preset = AI_PROVIDER_PRESETS.find((item) => item.id === value)
                if (preset) {
                  applyAiPreset(preset)
                }
              }}
            />
            <p className="ai-settings-preset-hint">{t.aiSettingsPresetHint}</p>
          </div>
        )}

        <fieldset className="ai-settings-provider-fields" disabled={!desktopApi || aiOperation !== null}>
          <div className="ai-settings-form">
            <label>
              <span>{t.aiSettingsProviderName}</span>
              <input
                placeholder={t.aiSettingsProviderNamePlaceholder}
                value={aiDraft.name}
                onChange={(event) => patchAiDraft({ name: event.target.value })}
              />
            </label>
            <label>
              <span>{t.aiSettingsProviderType}</span>
              <DropdownSelect
                disabled={!desktopApi || aiOperation !== null}
                value={aiDraft.kind}
                options={[
                  { value: 'openai-compatible-chat', label: 'OpenAI-compatible Chat (OpenAI 兼容对话协议)' },
                  { value: 'openai-responses', label: 'OpenAI Responses (OpenAI 官方结构化响应协议)' },
                  { value: 'anthropic-messages', label: 'Anthropic Messages (Claude 官方消息协议)' }
                ]}
                onChange={(value) => patchAiDraft({ kind: value as AiProviderKind })}
              />
            </label>
            <label className="ai-settings-model-field">
              <div className="ai-settings-model-header">
                <span>{t.aiSettingsModel}</span>
                <small className="ai-settings-model-picker-hint">{t.aiSettingsModelAddHint}</small>
              </div>
              <div className="ai-settings-model-container">
                <div className="ai-settings-model-left">
                  {isCustomInput ? (
                    <div className="ai-settings-model-add-row">
                      <input
                        autoFocus
                        className="ai-settings-model-add-input"
                        placeholder="输入自定义模型名称 (例: deepseek-r1)"
                        value={customModelText}
                        onChange={(e) => setCustomModelText(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault()
                            addSelectedModelToProvider()
                          } else if (e.key === 'Escape') {
                            setIsCustomInput(false)
                          }
                        }}
                      />
                      <button
                        className="ai-settings-secondary-button ai-settings-add-model-btn"
                        disabled={!customModelText.trim()}
                        type="button"
                        onClick={addSelectedModelToProvider}
                        title="添加此自定义模型到 Provider"
                      >
                        <AppIcon name="plus" size={14} />
                      </button>
                      <button
                        className="ai-settings-secondary-button ai-settings-model-cancel-btn"
                        type="button"
                        onClick={() => setIsCustomInput(false)}
                        title="取消"
                      >
                        取消
                      </button>
                    </div>
                  ) : (
                    <div className="ai-settings-model-picker-row">
                      <DropdownSelect
                        className="ai-settings-model-select"
                        disabled={!desktopApi || aiOperation !== null}
                        value={selectedCandidateModel || '__none__'}
                        options={[
                          { value: '__none__', label: '选择模型以添加...' },
                          ...candidateModelOptions.map((model: string) => ({
                            value: model,
                            label: model
                          })),
                          { value: '__custom__', label: '+ 自定义模型...' }
                        ]}
                        onChange={(value) => {
                          if (value === '__custom__') {
                            setIsCustomInput(true)
                            setSelectedCandidateModel('')
                            setCustomModelText('')
                          } else if (value === '__none__') {
                            setSelectedCandidateModel('')
                          } else {
                            setSelectedCandidateModel(value)
                            setIsCustomInput(false)
                          }
                        }}
                      />
                      <button
                        className="ai-settings-secondary-button ai-settings-add-model-btn"
                        disabled={!desktopApi || aiOperation !== null || !selectedCandidateModel}
                        type="button"
                        onClick={addSelectedModelToProvider}
                        title="添加当前下拉框选中的模型到 Provider"
                      >
                        <AppIcon name="plus" size={14} />
                      </button>
                    </div>
                  )}
                </div>
                <div className="ai-settings-model-right">
                  <span className="ai-settings-model-right-title">{t.aiSettingsConfiguredModelsTitle}</span>
                  {configuredModels.length > 0 ? (
                    <>
                      <div className="ai-settings-model-tags">
                        {configuredModels.map((modelName) => {
                          const isActive = aiDraft.model === modelName
                          return (
                            <span
                              key={modelName}
                              className={`ai-settings-model-tag ${isActive ? 'is-active' : ''}`}
                              onClick={() => patchAiDraft({ model: modelName })}
                              title={isActive ? '当前生效模型' : `点击作为当前生效模型 (${modelName})`}
                            >
                              <span className="ai-settings-model-tag-text">{modelName}</span>
                              <button
                                type="button"
                                className="ai-settings-model-tag-remove"
                                onClick={(e) => {
                                  e.stopPropagation()
                                  removeConfiguredModel(modelName)
                                }}
                                title="从此 Provider 移除该模型"
                              >
                                <AppIcon name="close" size={10} />
                              </button>
                            </span>
                          )
                        })}
                      </div>
                      <small className="ai-settings-model-default-hint">{t.aiSettingsModelDefaultHint}</small>
                    </>
                  ) : (
                    <div className="ai-settings-model-empty-hint">{t.aiSettingsModelEmptyHint}</div>
                  )}
                </div>
              </div>
            </label>
            <label className="ai-settings-form-span-two">
              <span>{t.aiSettingsEndpoint}</span>
              <input
                placeholder={t.aiSettingsEndpointPlaceholder}
                value={aiDraft.baseUrl}
                onChange={(event) => patchAiDraft({ baseUrl: event.target.value })}
              />
            </label>
            <div className="ai-settings-form-span-two ai-settings-form-field">
              <div className="ai-settings-api-key-header">
                <span>{t.aiSettingsApiKey}</span>
                {selectedAiProvider?.hasApiKey && !clearAiApiKey ? (
                  <button
                    type="button"
                    className="ai-settings-clear-key-btn"
                    onClick={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      setClearAiApiKey(true)
                      setAiApiKey('')
                    }}
                    title={t.aiSettingsClearApiKeyHint}
                  >
                    <AppIcon name="trash" size={13} />
                    {t.aiSettingsClearApiKey}
                  </button>
                ) : clearAiApiKey ? (
                  <span className="ai-settings-key-cleared-tag">
                    保存时将清除 Key
                    <button
                      type="button"
                      className="ai-settings-undo-clear-btn"
                      onClick={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        setClearAiApiKey(false)
                      }}
                    >
                      撤销
                    </button>
                  </span>
                ) : null}
              </div>
              <input
                autoComplete="off"
                disabled={clearAiApiKey}
                placeholder={
                  clearAiApiKey
                    ? '已标记保存时清除已保存的 API Key'
                    : selectedAiProvider?.hasApiKey
                      ? t.aiSettingsApiKeyReplacePlaceholder
                      : t.aiSettingsApiKeyPlaceholder
                }
                type="password"
                value={aiApiKey}
                onChange={(event) => {
                  setAiApiKey(event.target.value)
                  setClearAiApiKey(false)
                }}
              />
            </div>
          </div>

          <div className="ai-settings-toggle-list">
            <label className="ai-settings-toggle-row ssh-checkbox">
              <SelectionControl
                checked={aiDraft.enabled}
                type="checkbox"
                onChange={(event) => patchAiDraft({ enabled: event.target.checked })}
              />
              <span>
                <strong>{t.aiSettingsEnabled}</strong>
                <small>{t.aiSettingsEnabledHint}</small>
              </span>
            </label>
            <label className="ai-settings-toggle-row ssh-checkbox">
              <SelectionControl
                checked={aiDraft.isDefault}
                type="checkbox"
                onChange={(event) => patchAiDraft({ isDefault: event.target.checked })}
              />
              <span>
                <strong>{t.aiSettingsDefaultProvider}</strong>
                <small>{t.aiSettingsDefaultProviderHint}</small>
              </span>
            </label>
            <label className="ai-settings-toggle-row ssh-checkbox">
              <SelectionControl
                checked={aiDraft.allowNoAuth}
                type="checkbox"
                onChange={(event) => patchAiDraft({ allowNoAuth: event.target.checked })}
              />
              <span>
                <strong>{t.aiSettingsAllowNoAuth}</strong>
                <small>{t.aiSettingsAllowNoAuthHint}</small>
              </span>
            </label>
            <label className="ai-settings-toggle-row ssh-checkbox">
              <SelectionControl
                checked={aiDraft.allowInsecureHttp}
                type="checkbox"
                onChange={(event) => patchAiDraft({ allowInsecureHttp: event.target.checked })}
              />
              <span>
                <strong>{t.aiSettingsAllowInsecureHttp}</strong>
                <small>{t.aiSettingsAllowInsecureHttpHint}</small>
              </span>
            </label>
          </div>
        </fieldset>

        {aiDraft.allowInsecureHttp ? (
          <p className="ai-settings-warning" role="alert">
            {t.aiSettingsInsecureHttpWarning}
          </p>
        ) : null}

        <div className="ai-settings-privacy-card">
          <AppIcon name="key" size={16} />
          <div>
            <strong>{t.aiSettingsPrivacyTitle}</strong>
            <p>{t.aiSettingsPrivacyDescription}</p>
          </div>
        </div>

        <div className="ai-settings-footer">
          <small
            className={
              aiMessage ? `ai-settings-operation-message ai-settings-operation-message--${aiMessage.kind}` : undefined
            }
            role={aiMessage?.kind === 'error' ? 'alert' : 'status'}
          >
            {aiMessage?.message ?? t.aiSettingsConnectionTestHint}
          </small>
          <div className="ai-settings-footer-actions">
            {aiDraft.id ? (
              <button
                aria-busy={aiOperation === 'delete'}
                className="ai-settings-danger-button"
                disabled={!desktopApi || aiOperation !== null}
                type="button"
                onClick={() => setShowDeleteAiProviderConfirm(true)}
              >
                <AppIcon name="trash" size={14} />
                <span className="ai-settings-action-label">
                  <span>{aiOperation === 'delete' ? t.aiSettingsDeleting : t.aiSettingsDelete}</span>
                  <span aria-hidden="true" className="ai-settings-action-label-reserve">
                    {t.aiSettingsDelete}
                  </span>
                  <span aria-hidden="true" className="ai-settings-action-label-reserve">
                    {t.aiSettingsDeleting}
                  </span>
                </span>
              </button>
            ) : null}
            <button
              aria-busy={aiOperation === 'test'}
              className="ai-settings-secondary-button"
              disabled={!desktopApi || aiOperation !== null}
              type="button"
              onClick={() => void testAiProvider()}
            >
              <AppIcon name="flash" size={14} />
              <span className="ai-settings-action-label">
                <span>{aiOperation === 'test' ? t.aiSettingsTesting : t.aiSettingsTestConnection}</span>
                <span aria-hidden="true" className="ai-settings-action-label-reserve">
                  {t.aiSettingsTestConnection}
                </span>
                <span aria-hidden="true" className="ai-settings-action-label-reserve">
                  {t.aiSettingsTesting}
                </span>
              </span>
            </button>
            <button
              aria-busy={aiOperation === 'save'}
              className="primary-button compact"
              disabled={!desktopApi || aiOperation !== null}
              type="button"
              onClick={() => void saveAiProvider()}
            >
              <AppIcon name="disk" size={14} />
              <span className="ai-settings-action-label">
                <span>{aiOperation === 'save' ? t.aiSettingsSaving : t.aiSettingsSave}</span>
                <span aria-hidden="true" className="ai-settings-action-label-reserve">
                  {t.aiSettingsSave}
                </span>
                <span aria-hidden="true" className="ai-settings-action-label-reserve">
                  {t.aiSettingsSaving}
                </span>
              </span>
            </button>
          </div>
        </div>
        {showDeleteAiProviderConfirm ? (
          <ConfirmActionDialog
            confirmLabel={t.delete}
            confirmVariant="danger"
            description={`确定要删除 Provider "${aiDraft.name || aiDraft.id}" 吗？删除后不可恢复。`}
            isSubmitting={aiOperation === 'delete'}
            onClose={() => {
              if (aiOperation !== 'delete') {
                setShowDeleteAiProviderConfirm(false)
              }
            }}
            onConfirm={() => void deleteAiProvider()}
            title="删除 Provider 确认"
          />
        ) : null}
      </section>
    </div>
  )
}
