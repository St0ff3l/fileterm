import type {
  ConnectionProfile,
  CreateProfileInput,
  ResourceMonitoringMetric,
  SshConnectionDefaults
} from '@fileterm/core'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { ResourceMonitoringMetricsEditor } from '../common/resource-monitoring-metrics-editor'
import { SelectionControl } from '../common/selection-control'
import {
  effectiveConnectionSetting,
  type ConnectionFormSetter,
  type SshConnectionSettingKey
} from './connection-modal-utils'

export function ConnectionSshAdvancedSection({
  connectionDefaults,
  fallbackResourceMonitoringMetrics,
  fallbackResourceMonitoringMetricOrder,
  form,
  intervalSettingOptions,
  isNetworkDevice,
  isSubmitting,
  jumpHosts,
  routingMode,
  setForm,
  setRoutingMode,
  setSshConnectionSetting
}: {
  connectionDefaults: SshConnectionDefaults
  fallbackResourceMonitoringMetrics?: ResourceMonitoringMetric[]
  fallbackResourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  form: CreateProfileInput
  intervalSettingOptions: Array<{ value: string; label: string }>
  isNetworkDevice: boolean
  isSubmitting: boolean
  jumpHosts: ConnectionProfile[]
  routingMode: 'direct' | 'jump'
  setForm: ConnectionFormSetter
  setRoutingMode(value: 'direct' | 'jump'): void
  setSshConnectionSetting<K extends SshConnectionSettingKey>(key: K, value: SshConnectionDefaults[K]): void
}) {
  return (
    <>
      {form.type === 'ssh' ? (
        <fieldset className="ssh-fieldset">
          <legend>{t.advanced}</legend>
          <div className="advanced-toggle-list">
            {form.authType === 'password' ? (
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={effectiveConnectionSetting(form, connectionDefaults, 'useEmptyPassword')}
                    type="checkbox"
                    onChange={(event) => {
                      const nextValue = event.target.checked
                      setSshConnectionSetting('useEmptyPassword', nextValue)
                      if (nextValue) {
                        setForm((previous) => ({ ...previous, password: '' }))
                      }
                    }}
                  />
                  <span className="advanced-toggle-name">{t.useEmptyPassword}</span>
                </label>
                <p className="advanced-toggle-hint">{t.useEmptyPasswordHint}</p>
              </div>
            ) : null}
            {!isNetworkDevice ? (
              <>
                <div className="advanced-toggle-row">
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={effectiveConnectionSetting(form, connectionDefaults, 'enableExecChannel')}
                      type="checkbox"
                      onChange={(event) => setSshConnectionSetting('enableExecChannel', event.target.checked)}
                    />
                    <span className="advanced-toggle-name">{t.enableExecChannel}</span>
                  </label>
                  <p className="advanced-toggle-hint">{t.enableExecChannelHint}</p>
                </div>
                <div className="advanced-toggle-row">
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring')}
                      type="checkbox"
                      onChange={(event) => setSshConnectionSetting('enableResourceMonitoring', event.target.checked)}
                    />
                    <span className="advanced-toggle-name">{t.resourceMonitoring}</span>
                  </label>
                  <p className="advanced-toggle-hint">{t.resourceMonitoringDescription}</p>
                  <label className="resource-monitoring-interval">
                    <span>{t.resourceMonitoringInterval}</span>
                    <DropdownSelect
                      className="resource-monitoring-interval__select"
                      disabled={!effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring')}
                      options={intervalSettingOptions}
                      value={String(
                        effectiveConnectionSetting(form, connectionDefaults, 'resourceMonitoringIntervalSeconds')
                      )}
                      onChange={(value) =>
                        setSshConnectionSetting(
                          'resourceMonitoringIntervalSeconds',
                          Number(value) as SshConnectionDefaults['resourceMonitoringIntervalSeconds']
                        )
                      }
                    />
                  </label>
                  {form.type === 'ssh' ? (
                    <ResourceMonitoringMetricsEditor
                      metrics={
                        form.resourceMonitoringMetrics ??
                        fallbackResourceMonitoringMetrics ??
                        connectionDefaults.resourceMonitoringMetrics
                      }
                      order={
                        form.resourceMonitoringMetricOrder ??
                        fallbackResourceMonitoringMetricOrder ??
                        connectionDefaults.resourceMonitoringMetricOrder
                      }
                      disabled={
                        !effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring') ||
                        isSubmitting
                      }
                      onMetricsChange={(next) => setSshConnectionSetting('resourceMonitoringMetrics', next)}
                      onOrderChange={(next) => setSshConnectionSetting('resourceMonitoringMetricOrder', next)}
                    />
                  ) : null}
                </div>
                <div className="advanced-toggle-row">
                  <label className="ssh-checkbox advanced-toggle-label">
                    <SelectionControl
                      checked={form.sftpEnabled !== false}
                      type="checkbox"
                      onChange={(event) => setForm((prev) => ({ ...prev, sftpEnabled: event.target.checked }))}
                    />
                    <span className="advanced-toggle-name">{t.sftpEnabled}</span>
                  </label>
                  <p className="advanced-toggle-hint">{t.sftpEnabledHint}</p>
                </div>
              </>
            ) : (
              <div className="advanced-toggle-row">
                <p className="advanced-toggle-hint">{t.networkDeviceCapabilitiesHint}</p>
              </div>
            )}
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={effectiveConnectionSetting(form, connectionDefaults, 'legacyAlgorithms')}
                  type="checkbox"
                  onChange={(event) => setSshConnectionSetting('legacyAlgorithms', event.target.checked)}
                />
                <span className="advanced-toggle-name">{t.legacyAlgorithms}</span>
              </label>
              <p className="advanced-toggle-hint">{t.legacyAlgorithmsHint}</p>
            </div>
          </div>
          <div className="reconnect-mode-group">
            <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
            <div className="advanced-toggle-list">
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'none'}
                    name="connection-reconnect-mode"
                    type="radio"
                    onChange={() => setSshConnectionSetting('reconnectMode', 'none')}
                  />
                  <span className="advanced-toggle-name">{t.reconnectNone}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'enter'}
                    name="connection-reconnect-mode"
                    type="radio"
                    onChange={() => setSshConnectionSetting('reconnectMode', 'enter')}
                  />
                  <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'auto'}
                    name="connection-reconnect-mode"
                    type="radio"
                    onChange={() => setSshConnectionSetting('reconnectMode', 'auto')}
                  />
                  <span className="advanced-toggle-name">{t.autoReconnect}</span>
                </label>
                <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                {effectiveConnectionSetting(form, connectionDefaults, 'reconnectMode') === 'auto' ? (
                  <div className="reconnect-auto-limit-box">
                    <label className="reconnect-auto-limit-label">
                      <span>{t.reconnectMaxAttempts}:</span>
                      <input
                        inputMode="numeric"
                        min={0}
                        max={4294967295}
                        type="number"
                        value={form.reconnectMaxAttempts ?? 0}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            reconnectMaxAttempts: Math.max(0, Math.min(4294967295, Number(event.target.value) || 0))
                          }))
                        }
                      />
                    </label>
                    <span className="reconnect-auto-limit-hint">{t.reconnectMaxAttemptsHint}</span>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
          <div className="reconnect-mode-group">
            <div className="reconnect-mode-group__label">{t.timeoutAndKeepalive}</div>
            <div className="advanced-toggle-list">
              <div className="advanced-toggle-row">
                <div className="reliability-inputs-grid">
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.connectionTimeout}:</span>
                    <input
                      inputMode="numeric"
                      min={5}
                      max={300}
                      type="number"
                      value={form.connectTimeoutSeconds ?? 30}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          connectTimeoutSeconds: Math.max(5, Math.min(300, Number(event.target.value) || 5))
                        }))
                      }
                    />
                  </label>
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.operationTimeout}:</span>
                    <input
                      inputMode="numeric"
                      min={5}
                      max={3600}
                      type="number"
                      value={form.operationTimeoutSeconds ?? 60}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          operationTimeoutSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                        }))
                      }
                    />
                  </label>
                </div>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.keepaliveEnabled !== false}
                    type="checkbox"
                    onChange={(event) => setForm((prev) => ({ ...prev, keepaliveEnabled: event.target.checked }))}
                  />
                  <span className="advanced-toggle-name">{t.keepalive}</span>
                </label>
                <p className="advanced-toggle-hint">{t.keepaliveHint}</p>
                {form.keepaliveEnabled !== false ? (
                  <div className="reliability-inputs-grid">
                    <label className="reliability-field-box">
                      <span className="reliability-field-label">{t.keepaliveInterval}:</span>
                      <input
                        inputMode="numeric"
                        min={5}
                        max={3600}
                        type="number"
                        value={form.keepaliveIntervalSeconds ?? 30}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            keepaliveIntervalSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                          }))
                        }
                      />
                    </label>
                    <label className="reliability-field-box">
                      <span className="reliability-field-label">{t.keepaliveMaxMisses}:</span>
                      <input
                        inputMode="numeric"
                        min={1}
                        max={32}
                        type="number"
                        value={form.keepaliveMaxMisses ?? 3}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            keepaliveMaxMisses: Math.max(1, Math.min(32, Number(event.target.value) || 1))
                          }))
                        }
                      />
                    </label>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
          <div className="reconnect-mode-group network-routing-group">
            <div className="reconnect-mode-group__label network-routing-group__label">
              <span aria-hidden="true" className="material-symbols-outlined">
                account_tree
              </span>
              {t.networkRouting}
            </div>
            <div className="advanced-toggle-list network-routing-list">
              <div className="advanced-toggle-row network-routing-row">
                <span className="network-routing-row__name">{t.route}</span>
                <div className="network-routing-modes" role="radiogroup" aria-label={t.route}>
                  <button
                    aria-checked={routingMode === 'direct'}
                    className={routingMode === 'direct' ? 'is-active' : undefined}
                    onClick={() => {
                      setRoutingMode('direct')
                      setForm((prev) => ({ ...prev, jumpProfileId: undefined }))
                    }}
                    role="radio"
                    type="button"
                  >
                    {t.direct}
                  </button>
                  <button
                    aria-checked={routingMode === 'jump'}
                    className={routingMode === 'jump' ? 'is-active' : undefined}
                    onClick={() => setRoutingMode('jump')}
                    role="radio"
                    type="button"
                  >
                    {t.viaJumpHost}
                  </button>
                </div>
              </div>
              {routingMode === 'jump' && jumpHosts.length ? (
                <>
                  <label className="advanced-toggle-row network-routing-row">
                    <span className="network-routing-row__name">{t.jumpHost}</span>
                    <DropdownSelect
                      className="network-routing-select"
                      placeholder={t.selectJumpHost}
                      value={form.jumpProfileId ?? ''}
                      options={[
                        { value: '', label: t.selectJumpHost, disabled: true },
                        ...jumpHosts.map((profile) => ({
                          value: profile.id,
                          label: `${profile.name} (${profile.host})`
                        }))
                      ]}
                      onChange={(value) => setForm((prev) => ({ ...prev, jumpProfileId: value || undefined }))}
                    />
                  </label>
                  <p className="network-routing-hint">{t.jumpHostHint}</p>
                </>
              ) : null}
              {routingMode === 'jump' && !jumpHosts.length ? (
                <p className="network-routing-empty">{t.noAvailableJumpHost}</p>
              ) : null}
            </div>
          </div>
        </fieldset>
      ) : null}
      {form.type === 'ftp' || form.type === 'telnet' ? (
        <fieldset className="ssh-fieldset">
          <legend>{t.advanced}</legend>
          <div className="reconnect-mode-group">
            <div className="reconnect-mode-group__label">{t.disconnectBehavior}</div>
            <div className="advanced-toggle-list">
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={(form.reconnectMode ?? 'none') === 'none'}
                    name="network-reconnect-mode"
                    type="radio"
                    onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'none' }))}
                  />
                  <span className="advanced-toggle-name">{t.reconnectNone}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.reconnectMode === 'enter'}
                    name="network-reconnect-mode"
                    type="radio"
                    onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'enter' }))}
                  />
                  <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.reconnectMode === 'auto'}
                    name="network-reconnect-mode"
                    type="radio"
                    onChange={() => setForm((prev) => ({ ...prev, reconnectMode: 'auto' }))}
                  />
                  <span className="advanced-toggle-name">{t.autoReconnect}</span>
                </label>
                <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
                {form.reconnectMode === 'auto' ? (
                  <div className="reconnect-auto-limit-box">
                    <label className="reconnect-auto-limit-label">
                      <span>{t.reconnectMaxAttempts}:</span>
                      <input
                        inputMode="numeric"
                        min={0}
                        max={4294967295}
                        type="number"
                        value={form.reconnectMaxAttempts ?? 0}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            reconnectMaxAttempts: Math.max(0, Math.min(4294967295, Number(event.target.value) || 0))
                          }))
                        }
                      />
                    </label>
                    <span className="reconnect-auto-limit-hint">{t.reconnectMaxAttemptsHint}</span>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
          <div className="reconnect-mode-group">
            <div className="reconnect-mode-group__label">{t.timeoutAndKeepalive}</div>
            <div className="advanced-toggle-list">
              <div className="advanced-toggle-row">
                <div className="reliability-inputs-grid">
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.connectionTimeout}:</span>
                    <input
                      inputMode="numeric"
                      min={5}
                      max={300}
                      type="number"
                      value={form.connectTimeoutSeconds ?? 30}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          connectTimeoutSeconds: Math.max(5, Math.min(300, Number(event.target.value) || 5))
                        }))
                      }
                    />
                  </label>
                  <label className="reliability-field-box">
                    <span className="reliability-field-label">{t.operationTimeout}:</span>
                    <input
                      inputMode="numeric"
                      min={5}
                      max={3600}
                      type="number"
                      value={form.operationTimeoutSeconds ?? 60}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          operationTimeoutSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                        }))
                      }
                    />
                  </label>
                </div>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={form.keepaliveEnabled !== false}
                    type="checkbox"
                    onChange={(event) => setForm((prev) => ({ ...prev, keepaliveEnabled: event.target.checked }))}
                  />
                  <span className="advanced-toggle-name">{t.keepalive}</span>
                </label>
                <p className="advanced-toggle-hint">{t.keepaliveHint}</p>
                {form.keepaliveEnabled !== false ? (
                  <div className="reliability-inputs-grid">
                    <label className="reliability-field-box">
                      <span className="reliability-field-label">{t.keepaliveInterval}:</span>
                      <input
                        inputMode="numeric"
                        min={5}
                        max={3600}
                        type="number"
                        value={form.keepaliveIntervalSeconds ?? 30}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            keepaliveIntervalSeconds: Math.max(5, Math.min(3600, Number(event.target.value) || 5))
                          }))
                        }
                      />
                    </label>
                    <label className="reliability-field-box">
                      <span className="reliability-field-label">{t.keepaliveMaxMisses}:</span>
                      <input
                        inputMode="numeric"
                        min={1}
                        max={32}
                        type="number"
                        value={form.keepaliveMaxMisses ?? 3}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            keepaliveMaxMisses: Math.max(1, Math.min(32, Number(event.target.value) || 1))
                          }))
                        }
                      />
                    </label>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </fieldset>
      ) : null}
    </>
  )
}
