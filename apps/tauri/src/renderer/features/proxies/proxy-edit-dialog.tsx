import { useEffect, useState, type FormEvent } from 'react'
import { createPortal } from 'react-dom'
import type { CreateProxyProfileInput, ProxyProfile, UpdateProxyProfileInput } from '@fileterm/core'
import { AppIcon } from '../common/app-icon'
import { CloseButton } from '../common/close-button'
import { DropdownSelect } from '../common/dropdown-select'
import { FeedbackText } from '../common/feedback-text'
import { StableButtonContent } from '../common/stable-button-content'
import { t } from '../../i18n'

export function ProxyEditDialog({
  initialProxy,
  initialType = 'socks5',
  isOpen,
  isSubmitting,
  standalone = false,
  onClose,
  onSave,
  onTest
}: {
  initialProxy?: ProxyProfile | null
  initialType?: 'socks5' | 'http'
  isOpen: boolean
  isSubmitting: boolean
  standalone?: boolean
  onClose(): void
  onSave(input: CreateProxyProfileInput | UpdateProxyProfileInput): Promise<void>
  onTest(input: CreateProxyProfileInput): Promise<{ success: boolean; latencyMs?: number; error?: string }>
}) {
  const isEdit = Boolean(initialProxy?.id)
  const [activeSection, setActiveSection] = useState<'general' | 'connections' | 'test'>('general')
  const [name, setName] = useState(initialProxy?.name ?? '')
  const [proxyType, setProxyType] = useState<'socks5' | 'http'>(initialProxy?.type ?? initialType)
  const [host, setHost] = useState(initialProxy?.host ?? '127.0.0.1')
  const [port, setPort] = useState(initialProxy?.port ? String(initialProxy.port) : '1080')
  const [username, setUsername] = useState(initialProxy?.username ?? '')
  const [password, setPassword] = useState('')
  const [testTarget, setTestTarget] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [isTesting, setIsTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ success: boolean; latencyMs?: number; error?: string } | null>(null)

  useEffect(() => {
    if (!isOpen) return
    setActiveSection('general')
    setName(initialProxy?.name ?? '')
    setProxyType(initialProxy?.type ?? initialType)
    setHost(initialProxy?.host ?? '127.0.0.1')
    setPort(initialProxy?.port ? String(initialProxy.port) : '1080')
    setUsername(initialProxy?.username ?? '')
    setPassword('')
    setTestTarget('')
    setShowPassword(false)
    setErrorMessage(null)
    setIsTesting(false)
    setTestResult(null)
  }, [initialProxy?.id, initialType, isOpen])

  if (!isOpen) return null

  const handleTest = async () => {
    const trimmedHost = host.trim()
    const numericPort = Number(port)
    if (!trimmedHost) {
      setErrorMessage(`${t.proxyHost} 不能为空`)
      setActiveSection('general')
      return
    }
    if (!numericPort || numericPort < 1 || numericPort > 65535) {
      setErrorMessage(`${t.proxyPort} 无效 (1-65535)`)
      setActiveSection('general')
      return
    }

    setIsTesting(true)
    setTestResult(null)
    setErrorMessage(null)
    try {
      const res = await onTest({
        name: name.trim() || 'Test Proxy',
        type: proxyType,
        host: trimmedHost,
        port: numericPort,
        username: username.trim() || undefined,
        password: password || undefined
      })
      setTestResult(res)
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err))
    } finally {
      setIsTesting(false)
    }
  }

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    const trimmedName = name.trim()
    const trimmedHost = host.trim()
    const numericPort = Number(port)

    if (!trimmedName) {
      setErrorMessage(`${t.proxyName} 不能为空`)
      setActiveSection('general')
      return
    }
    if (!trimmedHost) {
      setErrorMessage(`${t.proxyHost} 不能为空`)
      setActiveSection('general')
      return
    }
    if (!numericPort || numericPort < 1 || numericPort > 65535) {
      setErrorMessage(`${t.proxyPort} 无效 (1-65535)`)
      setActiveSection('general')
      return
    }

    setErrorMessage(null)
    try {
      if (isEdit && initialProxy) {
        await onSave({
          id: initialProxy.id,
          name: trimmedName,
          type: proxyType,
          host: trimmedHost,
          port: numericPort,
          username: username.trim() || undefined,
          password: password || undefined
        })
      } else {
        await onSave({
          name: trimmedName,
          type: proxyType,
          host: trimmedHost,
          port: numericPort,
          username: username.trim() || undefined,
          password: password || undefined
        })
      }
      onClose()
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err))
    }
  }

  const isFormBusy = isSubmitting || isTesting
  const dialogTitle = isEdit ? (initialProxy?.name ? `${t.editProxy} - ${initialProxy.name}` : t.editProxy) : t.newProxy

  const boundConnections: string[] = initialProxy?.boundConnectionNames ?? []

  const content = (
    <div
      className={`modal-card ssh-modal proxy-edit-modal ${standalone ? 'standalone' : ''}`}
      role="dialog"
      aria-modal="true"
      onClick={(e) => e.stopPropagation()}
    >
      <div className="connection-manager-header" data-tauri-drag-region={standalone ? 'deep' : undefined}>
        <span className="connection-manager-title">
          <AppIcon name="shield" size={18} />
          <span>{dialogTitle}</span>
        </span>
        <div className="connection-manager-header-actions">
          <CloseButton disabled={isFormBusy} onClick={onClose} />
        </div>
      </div>

      <div className="ssh-modal-body">
        <aside className="ssh-modal-nav" aria-label="Proxy Navigation">
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
            {t.proxyBoundConnections}
            {boundConnections.length > 0 ? ` (${boundConnections.length})` : ''}
          </button>
          <button
            className={activeSection === 'test' ? 'active' : ''}
            type="button"
            onClick={() => setActiveSection('test')}
          >
            {t.connectivityTest}
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
                      {t.name}:
                      <input
                        type="text"
                        required
                        value={name}
                        placeholder={t.proxyNamePlaceholder}
                        onChange={(e) => {
                          setName(e.target.value)
                          setErrorMessage(null)
                        }}
                      />
                    </label>

                    <label className="span-2">
                      {t.proxyProtocol || '代理协议'}:
                      <DropdownSelect
                        value={proxyType}
                        options={[
                          { value: 'socks5', label: t.proxyProtocolSocks5 },
                          { value: 'http', label: t.proxyProtocolHttpConnect }
                        ]}
                        onChange={(val) => setProxyType(val as 'socks5' | 'http')}
                      />
                    </label>

                    <label>
                      {t.host}:
                      <input
                        type="text"
                        required
                        value={host}
                        placeholder="127.0.0.1"
                        onChange={(e) => {
                          setHost(e.target.value)
                          setErrorMessage(null)
                        }}
                      />
                    </label>

                    <label>
                      {t.port}:
                      <input
                        type="number"
                        required
                        min={1}
                        max={65535}
                        value={port}
                        onChange={(e) => {
                          setPort(e.target.value)
                          setErrorMessage(null)
                        }}
                      />
                    </label>
                  </div>
                </fieldset>

                <fieldset className="ssh-fieldset">
                  <legend>{t.auth}</legend>
                  <div className="ssh-grid">
                    <label className="span-2">
                      {t.username}:
                      <input
                        type="text"
                        value={username}
                        placeholder="可选，留空表示无认证"
                        onChange={(e) => setUsername(e.target.value)}
                      />
                    </label>

                    <label className="span-2">
                      {t.password}:
                      <div className="proxy-password-field">
                        <input
                          type={showPassword ? 'text' : 'password'}
                          value={password}
                          placeholder={isEdit ? '留空保持原有密码不变' : '可选，留空表示无认证'}
                          onChange={(e) => setPassword(e.target.value)}
                        />
                        <button
                          type="button"
                          className="proxy-pwd-toggle-btn"
                          onClick={() => setShowPassword(!showPassword)}
                          aria-label={showPassword ? '隐藏密码' : '显示密码'}
                          tabIndex={-1}
                        >
                          <AppIcon name={showPassword ? 'eye-off' : 'eye'} size={15} />
                        </button>
                      </div>
                    </label>

                    <div className="span-2 ssh-field-hint">{t.proxyAuthHint}</div>
                  </div>
                </fieldset>
              </div>
            ) : null}

            {activeSection === 'connections' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.proxyBoundConnections}</legend>
                  {boundConnections.length > 0 ? (
                    <div className="proxy-bound-list">
                      <div className="ssh-field-hint">以下连接已绑定当前代理配置：</div>
                      <div className="proxy-bound-tags">
                        {boundConnections.map((connName) => (
                          <div key={connName} className="proxy-bound-card">
                            <AppIcon name="connections" size={16} />
                            <span className="proxy-bound-name">{connName}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <div className="proxy-empty-connections-hint">
                      <AppIcon name="info" size={24} />
                      <div>暂无关联此代理的连接配置。</div>
                      <div className="ssh-field-hint">
                        您可以在任意新建或编辑连接的「网络与代理」选项中，直接选用此代理。
                      </div>
                    </div>
                  )}
                </fieldset>
              </div>
            ) : null}

            {activeSection === 'test' ? (
              <div className="ssh-form-page">
                <fieldset className="ssh-fieldset">
                  <legend>{t.connectivityTest}</legend>
                  <div className="ssh-grid">
                    <label className="span-2">
                      {t.proxyTestTarget}:
                      <input
                        type="text"
                        value={testTarget}
                        placeholder={t.proxyTestTargetHint}
                        onChange={(e) => setTestTarget(e.target.value)}
                      />
                    </label>
                    <div className="span-2 ssh-field-hint">
                      默认直接测试代理服务器握手响应；亦可输入指定探测目标主机与端口。
                    </div>

                    <div className="span-2 proxy-test-result-wrapper">
                      {isTesting ? (
                        <div className="proxy-testing-banner">
                          <span className="button-spinner" />
                          <span>{t.testingProxy}</span>
                        </div>
                      ) : testResult ? (
                        <div className={`proxy-result-card ${testResult.success ? 'is-success' : 'is-error'}`}>
                          <AppIcon name={testResult.success ? 'check' : 'info'} size={18} />
                          <div className="proxy-result-detail">
                            <strong>{testResult.success ? t.proxyTestSuccess : t.proxyTestFailed}</strong>
                            <span>
                              {testResult.success
                                ? `目标握手延迟: ${testResult.latencyMs ?? 0}ms`
                                : testResult.error || '无法连接到该代理服务器'}
                            </span>
                          </div>
                        </div>
                      ) : (
                        <div className="proxy-test-empty-guide">
                          <AppIcon name="shield" size={24} />
                          <span>点击右下方「测试」按钮对当前代理服务器配置进行实时连通性探测。</span>
                        </div>
                      )}
                    </div>
                  </div>
                </fieldset>
              </div>
            ) : null}

            <div className="form-actions ssh-actions">
              {testResult ? (
                <FeedbackText
                  className="connection-test-feedback"
                  message={
                    testResult.success
                      ? `✓ ${t.proxyTestSuccess} (${testResult.latencyMs ?? 0}ms)`
                      : testResult.error || t.proxyTestFailed
                  }
                  tone={testResult.success ? 'success' : 'error'}
                />
              ) : errorMessage ? (
                <FeedbackText className="connection-test-feedback" message={errorMessage} tone="error" />
              ) : null}
              <button className="flat-button" disabled={isFormBusy} onClick={onClose} type="button">
                {t.cancel}
              </button>
              <button
                aria-busy={isTesting}
                className="flat-button"
                disabled={isFormBusy}
                onClick={() => void handleTest()}
                type="button"
              >
                <StableButtonContent busy={isTesting} label={t.test} />
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

  if (standalone) {
    return <div className="connection-form-window">{content}</div>
  }

  return createPortal(
    <div className="modal-backdrop" onClick={isFormBusy ? undefined : onClose}>
      {content}
    </div>,
    document.body
  )
}
