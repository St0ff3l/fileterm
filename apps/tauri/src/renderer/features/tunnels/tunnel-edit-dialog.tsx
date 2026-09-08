import { useEffect, useState, type FormEvent } from 'react'
import { createPortal } from 'react-dom'
import type {
  CreateTunnelProfileInput,
  SshForwardRule,
  TunnelProfile,
  TunnelProfileType,
  UpdateTunnelProfileInput
} from '@fileterm/core'
import { AppIcon } from '../common/app-icon'
import { CloseButton } from '../common/close-button'
import { DropdownSelect } from '../common/dropdown-select'
import { FeedbackText } from '../common/feedback-text'
import { StableButtonContent } from '../common/stable-button-content'
import { TunnelRuleEditor } from '../connections/tunnel-rule-editor'
import { t } from '../../i18n'

function createForwardRule(): SshForwardRule {
  return {
    id: globalThis.crypto.randomUUID(),
    kind: 'local',
    bindHost: '127.0.0.1',
    bindPort: 8080,
    targetHost: '127.0.0.1',
    targetPort: 22,
    autoStart: true
  }
}

function initialForwards(tunnel?: TunnelProfile | null, type: TunnelProfileType = 'ssh') {
  if (type !== 'ssh') return []
  return tunnel?.forwards?.map((rule) => ({ ...rule })) ?? [createForwardRule()]
}

function portIsValid(value: number, allowZero = false) {
  return Number.isInteger(value) && value >= (allowZero ? 0 : 1) && value <= 65535
}

export function TunnelEditDialog({
  initialTunnel,
  initialType = 'ssh',
  isOpen,
  isSubmitting,
  standalone = false,
  onClose,
  onSave
}: {
  initialTunnel?: TunnelProfile | null
  initialType?: TunnelProfileType
  isOpen: boolean
  isSubmitting: boolean
  standalone?: boolean
  onClose(): void
  onSave(input: CreateTunnelProfileInput | UpdateTunnelProfileInput): Promise<void>
}) {
  const [activeSection, setActiveSection] = useState<'general' | 'connections'>('general')
  const [name, setName] = useState(initialTunnel?.name ?? '')
  const [tunnelType, setTunnelType] = useState<TunnelProfileType>(initialTunnel?.type ?? initialType)
  const [forwards, setForwards] = useState<SshForwardRule[]>(
    initialForwards(initialTunnel, initialTunnel?.type ?? initialType)
  )
  const [scriptUrl, setScriptUrl] = useState(initialTunnel?.scriptUrl ?? '')
  const [token, setToken] = useState('')
  const [timeoutSeconds, setTimeoutSeconds] = useState(String(initialTunnel?.timeoutSeconds ?? 30))
  const [showToken, setShowToken] = useState(false)
  const [clearToken, setClearToken] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)

  const isEdit = Boolean(initialTunnel?.id)
  const boundConnections = initialTunnel?.boundConnectionNames ?? []

  useEffect(() => {
    if (!isOpen) return
    const type = initialTunnel?.type ?? initialType
    setActiveSection('general')
    setName(initialTunnel?.name ?? '')
    setTunnelType(type)
    setForwards(initialForwards(initialTunnel, type))
    setScriptUrl(initialTunnel?.scriptUrl ?? '')
    setToken('')
    setTimeoutSeconds(String(initialTunnel?.timeoutSeconds ?? 30))
    setShowToken(false)
    setClearToken(false)
    setErrorMessage(null)
  }, [initialTunnel?.id, initialType, isOpen])

  if (!isOpen) return null

  const updateForward = (id: string, patch: Partial<SshForwardRule>) => {
    setForwards((current) => current.map((rule) => (rule.id === id ? { ...rule, ...patch } : rule)))
  }

  const removeForward = (id: string) => {
    setForwards((current) => current.filter((rule) => rule.id !== id))
  }

  const handleTypeChange = (value: string) => {
    const nextType = value as TunnelProfileType
    setTunnelType(nextType)
    setErrorMessage(null)
    if (nextType === 'ssh' && forwards.length === 0) {
      setForwards([createForwardRule()])
    }
    if (nextType === 'http') {
      setForwards([])
    }
  }

  const validate = () => {
    const trimmedName = name.trim()
    if (!trimmedName) return t.tunnelProfileName + ' 不能为空'

    if (tunnelType === 'ssh') {
      if (forwards.length === 0) return '请至少添加一条 SSH 转发规则'
      for (const rule of forwards) {
        if (!rule.bindHost.trim() || !portIsValid(Number(rule.bindPort))) {
          return 'SSH 隧道监听地址或端口无效（端口范围 1-65535）'
        }
        if (rule.kind !== 'dynamic' && (!rule.targetHost?.trim() || !portIsValid(Number(rule.targetPort)))) {
          return 'SSH 隧道目标地址或端口无效（端口范围 1-65535）'
        }
      }
      return null
    }

    try {
      const url = new URL(scriptUrl.trim())
      if (!['http:', 'https:'].includes(url.protocol) || !url.hostname) return t.httpTunnelScriptUrl + ' 无效'
    } catch {
      return t.httpTunnelScriptUrl + ' 无效'
    }
    const timeout = Number(timeoutSeconds)
    if (!Number.isInteger(timeout) || timeout < 1 || timeout > 300) {
      return `${t.httpTunnelTimeout} 必须在 1-300 秒之间`
    }
    return null
  }

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    const validationError = validate()
    if (validationError) {
      setErrorMessage(validationError)
      setActiveSection('general')
      return
    }

    setErrorMessage(null)
    const input = {
      name: name.trim(),
      type: tunnelType,
      ...(tunnelType === 'ssh'
        ? {
            forwards: forwards.map((rule) => ({
              ...rule,
              bindHost: rule.bindHost.trim(),
              targetHost: rule.targetHost?.trim()
            }))
          }
        : {
            scriptUrl: scriptUrl.trim(),
            timeoutSeconds: Number(timeoutSeconds),
            ...(isEdit
              ? clearToken
                ? { token: null }
                : token.trim()
                  ? { token: token.trim() }
                  : {}
              : token.trim()
                ? { token: token.trim() }
                : {})
          })
    }

    try {
      await onSave(isEdit && initialTunnel ? { id: initialTunnel.id, ...input } : input)
      onClose()
    } catch (cause) {
      setErrorMessage(cause instanceof Error ? cause.message : String(cause))
    }
  }

  const isFormBusy = isSubmitting
  const dialogTitle = isEdit
    ? initialTunnel?.name
      ? `${t.editTunnel} - ${initialTunnel.name}`
      : t.editTunnel
    : tunnelType === 'http'
      ? t.newHttpTunnel
      : t.newSshTunnel

  const content = (
    <div
      aria-modal="true"
      className={`modal-card ssh-modal proxy-edit-modal tunnel-edit-modal ${standalone ? 'standalone' : ''}`}
      onClick={(event) => event.stopPropagation()}
      role="dialog"
    >
      <div className="connection-manager-header" data-tauri-drag-region={standalone ? 'deep' : undefined}>
        <span className="connection-manager-title">
          <AppIcon name="connections" size={18} />
          <span>{dialogTitle}</span>
        </span>
        <div className="connection-manager-header-actions">
          <CloseButton disabled={isFormBusy} onClick={onClose} />
        </div>
      </div>

      <div className="ssh-modal-body">
        <aside className="ssh-modal-nav" aria-label="Tunnel Navigation">
          <button
            className={activeSection === 'general' ? 'active' : ''}
            type="button"
            onClick={() => setActiveSection('general')}
          >
            {t.general}
          </button>
          <button
            className={activeSection === 'connections' ? 'active' : ''}
            type="button"
            onClick={() => setActiveSection('connections')}
          >
            {t.tunnelBoundConnections}
            {boundConnections.length > 0 ? ` (${boundConnections.length})` : ''}
          </button>
        </aside>

        <form aria-busy={isFormBusy} className="ssh-form-shell" onSubmit={handleSubmit}>
          <fieldset disabled={isFormBusy} style={{ border: 0, display: 'contents', margin: 0, padding: 0 }}>
            {activeSection === 'general' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.general}</legend>
                  <div className="ssh-grid ssh-grid-general">
                    <label className="span-2">
                      {t.tunnelProfileName}:
                      <input
                        required
                        type="text"
                        value={name}
                        placeholder={t.tunnelProfileNamePlaceholder}
                        onChange={(event) => {
                          setName(event.target.value)
                          setErrorMessage(null)
                        }}
                      />
                    </label>
                    <label className="span-2">
                      {t.tunnelProfileType}:
                      <DropdownSelect
                        value={tunnelType}
                        options={[
                          { value: 'ssh', label: t.sshTunnelProfiles },
                          { value: 'http', label: t.httpTunnels }
                        ]}
                        onChange={handleTypeChange}
                      />
                    </label>
                  </div>
                  <div className="tunnel-profile-note">
                    <AppIcon name="info" size={15} />
                    <span>{tunnelType === 'ssh' ? t.sshTunnelProfileHint : t.httpTunnelProfileHint}</span>
                  </div>
                </fieldset>

                {tunnelType === 'ssh' ? (
                  <fieldset className="ssh-fieldset tunnel-fieldset">
                    <legend>{t.tunnelForwardRules}</legend>
                    <div className="tunnel-rule-list">
                      {forwards.map((rule, index) => (
                        <TunnelRuleEditor
                          key={rule.id}
                          index={index}
                          rule={rule}
                          onChange={(patch) => updateForward(rule.id, patch)}
                          onRemove={() => removeForward(rule.id)}
                        />
                      ))}
                    </div>
                    <button
                      className="tunnel-add-button"
                      type="button"
                      onClick={() => setForwards((current) => [...current, createForwardRule()])}
                    >
                      <AppIcon name="plus" size={14} />
                      {t.addConnectionTunnel}
                    </button>
                  </fieldset>
                ) : (
                  <fieldset className="ssh-fieldset">
                    <legend>{t.httpTunnels}</legend>
                    <div className="ssh-grid">
                      <label className="span-2">
                        {t.httpTunnelScriptUrl}:
                        <input
                          required
                          type="url"
                          value={scriptUrl}
                          placeholder={t.httpTunnelScriptUrlPlaceholder}
                          onChange={(event) => {
                            setScriptUrl(event.target.value)
                            setErrorMessage(null)
                          }}
                        />
                      </label>
                      <label>
                        {t.httpTunnelTimeout}:
                        <input
                          required
                          min={1}
                          max={300}
                          type="number"
                          value={timeoutSeconds}
                          onChange={(event) => setTimeoutSeconds(event.target.value)}
                        />
                      </label>
                      <div className="tunnel-profile-note tunnel-profile-note--field">
                        <AppIcon name="info" size={15} />
                        <span>{t.httpTunnelTargetHint}</span>
                      </div>
                      <label className="span-2">
                        {t.httpTunnelToken}:
                        <div className="proxy-password-field">
                          <input
                            type={showToken ? 'text' : 'password'}
                            value={token}
                            placeholder={isEdit ? t.httpTunnelTokenPlaceholder : t.httpTunnelTokenPlaceholder}
                            onChange={(event) => {
                              setToken(event.target.value)
                              setClearToken(false)
                            }}
                          />
                          <button
                            aria-label={showToken ? '隐藏令牌' : '显示令牌'}
                            className="proxy-pwd-toggle-btn"
                            tabIndex={-1}
                            type="button"
                            onClick={() => setShowToken((visible) => !visible)}
                          >
                            <AppIcon name={showToken ? 'eye-off' : 'eye'} size={15} />
                          </button>
                        </div>
                      </label>
                      {isEdit && initialTunnel?.hasToken ? (
                        <label className="tunnel-profile-clear-token">
                          <input
                            type="checkbox"
                            checked={clearToken}
                            onChange={(event) => setClearToken(event.target.checked)}
                          />
                          {t.clearSavedToken}
                        </label>
                      ) : null}
                    </div>
                  </fieldset>
                )}
              </div>
            ) : null}

            {activeSection === 'connections' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.tunnelBoundConnections}</legend>
                  {boundConnections.length > 0 ? (
                    <div className="proxy-bound-list">
                      <div className="ssh-field-hint">{t.tunnelBoundHint}</div>
                      <div className="proxy-bound-tags">
                        {boundConnections.map((connectionName) => (
                          <div key={connectionName} className="proxy-bound-card">
                            <AppIcon name="connections" size={16} />
                            <span className="proxy-bound-name">{connectionName}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <div className="proxy-empty-connections-hint">
                      <AppIcon name="info" size={24} />
                      <div>{t.tunnelNoBoundConnections}</div>
                      <div className="ssh-field-hint">{t.tunnelEmptyHint}</div>
                    </div>
                  )}
                </fieldset>
              </div>
            ) : null}

            <div className="form-actions ssh-actions">
              <FeedbackText className="connection-test-feedback" message={errorMessage} tone="error" />
              <button className="flat-button" disabled={isFormBusy} type="button" onClick={onClose}>
                {t.cancel}
              </button>
              <button aria-busy={isSubmitting} className="primary-button" disabled={isFormBusy} type="submit">
                <StableButtonContent busy={isSubmitting} label={t.save} />
              </button>
            </div>
          </fieldset>
        </form>
      </div>
    </div>
  )

  if (standalone) return <div className="connection-form-window">{content}</div>

  return createPortal(
    <div className="modal-backdrop" onClick={isFormBusy ? undefined : onClose}>
      {content}
    </div>,
    document.body
  )
}
