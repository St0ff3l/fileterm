import type { CreateProfileInput, ResourceMonitoringMetric, SshConnectionDefaults } from '@fileterm/core'
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
  setForm,
  setSshConnectionSetting
}: {
  connectionDefaults: SshConnectionDefaults
  fallbackResourceMonitoringMetrics?: ResourceMonitoringMetric[]
  fallbackResourceMonitoringMetricOrder?: ResourceMonitoringMetric[]
  form: CreateProfileInput
  intervalSettingOptions: Array<{ value: string; label: string }>
  isNetworkDevice: boolean
  isSubmitting: boolean
  setForm: ConnectionFormSetter
  setSshConnectionSetting<K extends SshConnectionSettingKey>(key: K, value: SshConnectionDefaults[K]): void
}) {
  const isPasswordAuth = form.authType === 'password' || form.authType === 'kubernetes'

  return (
    <div className="ssh-form-page">
      <fieldset className="ssh-fieldset">
        <legend>{t.advancedSettings}</legend>
        <div className="advanced-toggle-list">
          {isPasswordAuth ? (
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
                    !effectiveConnectionSetting(form, connectionDefaults, 'enableResourceMonitoring') || isSubmitting
                  }
                  onMetricsChange={(next) => setSshConnectionSetting('resourceMonitoringMetrics', next)}
                  onOrderChange={(next) => setSshConnectionSetting('resourceMonitoringMetricOrder', next)}
                />
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
      </fieldset>
    </div>
  )
}
