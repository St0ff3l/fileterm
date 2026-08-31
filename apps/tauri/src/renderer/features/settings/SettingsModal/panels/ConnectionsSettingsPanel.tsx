import type { FileTermDesktopApi, SshConnectionDefaults } from '@fileterm/core'
import { DropdownSelect } from '../../../common/DropdownSelect'
import { ResourceMonitoringMetricsEditor } from '../../../common/ResourceMonitoringMetricsEditor'
import { SelectionControl } from '../../../common/SelectionControl'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type ConnectionsSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  connectionDefaults: SshConnectionDefaults
  isSavingConnectionDefaults: boolean
  setConnectionDefault<K extends keyof SshConnectionDefaults>(key: K, value: SshConnectionDefaults[K]): void
  connectionDefaultsError: string | null
}

export function ConnectionsSettingsPanel() {
  const {
    t,
    desktopApi,
    connectionDefaults,
    isSavingConnectionDefaults,
    setConnectionDefault,
    connectionDefaultsError
  } = useSettingsModalContext<ConnectionsSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.connectionDefaults}</h3>
        <p className="settings-tools-hint">{t.connectionDefaultsHint}</p>
        <fieldset
          className="settings-connection-defaults"
          disabled={!desktopApi || isSavingConnectionDefaults}
          style={{ border: 0, margin: 0, padding: 0 }}
        >
          <div className="advanced-toggle-list">
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={connectionDefaults.useEmptyPassword}
                  onChange={(event) => setConnectionDefault('useEmptyPassword', event.target.checked)}
                  type="checkbox"
                />
                <span className="advanced-toggle-name">{t.useEmptyPassword}</span>
              </label>
              <p className="advanced-toggle-hint">{t.useEmptyPasswordHint}</p>
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={connectionDefaults.enableExecChannel}
                  onChange={(event) => setConnectionDefault('enableExecChannel', event.target.checked)}
                  type="checkbox"
                />
                <span className="advanced-toggle-name">{t.enableExecChannel}</span>
              </label>
              <p className="advanced-toggle-hint">{t.enableExecChannelHint}</p>
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={connectionDefaults.enableResourceMonitoring}
                  onChange={(event) => setConnectionDefault('enableResourceMonitoring', event.target.checked)}
                  type="checkbox"
                />
                <span className="advanced-toggle-name">{t.resourceMonitoring}</span>
              </label>
              <p className="advanced-toggle-hint">{t.resourceMonitoringDescription}</p>
              <label className="resource-monitoring-interval">
                <span>{t.resourceMonitoringInterval}</span>
                <DropdownSelect
                  className="resource-monitoring-interval__select"
                  disabled={!connectionDefaults.enableResourceMonitoring}
                  options={[
                    { value: '1', label: t.resourceMonitoringEverySecond },
                    { value: '5', label: t.resourceMonitoringEvery5Seconds },
                    { value: '15', label: t.resourceMonitoringEvery15Seconds },
                    { value: '30', label: t.resourceMonitoringEvery30Seconds },
                    { value: '60', label: t.resourceMonitoringEvery60Seconds }
                  ]}
                  value={String(connectionDefaults.resourceMonitoringIntervalSeconds)}
                  onChange={(value) =>
                    setConnectionDefault(
                      'resourceMonitoringIntervalSeconds',
                      Number(value) as SshConnectionDefaults['resourceMonitoringIntervalSeconds']
                    )
                  }
                />
              </label>
              <ResourceMonitoringMetricsEditor
                metrics={connectionDefaults.resourceMonitoringMetrics}
                order={connectionDefaults.resourceMonitoringMetricOrder}
                disabled={!connectionDefaults.enableResourceMonitoring || isSavingConnectionDefaults}
                onMetricsChange={(next) => setConnectionDefault('resourceMonitoringMetrics', next)}
                onOrderChange={(next) => setConnectionDefault('resourceMonitoringMetricOrder', next)}
              />
            </div>
            <div className="advanced-toggle-row">
              <label className="ssh-checkbox advanced-toggle-label">
                <SelectionControl
                  checked={connectionDefaults.legacyAlgorithms}
                  onChange={(event) => setConnectionDefault('legacyAlgorithms', event.target.checked)}
                  type="checkbox"
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
                    checked={connectionDefaults.reconnectMode === 'none'}
                    name="global-reconnect-mode"
                    onChange={() => setConnectionDefault('reconnectMode', 'none')}
                    type="radio"
                  />
                  <span className="advanced-toggle-name">{t.reconnectNone}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectNoneHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={connectionDefaults.reconnectMode === 'enter'}
                    name="global-reconnect-mode"
                    onChange={() => setConnectionDefault('reconnectMode', 'enter')}
                    type="radio"
                  />
                  <span className="advanced-toggle-name">{t.reconnectEnter}</span>
                </label>
                <p className="advanced-toggle-hint">{t.reconnectEnterHint}</p>
              </div>
              <div className="advanced-toggle-row">
                <label className="ssh-checkbox advanced-toggle-label">
                  <SelectionControl
                    checked={connectionDefaults.reconnectMode === 'auto'}
                    name="global-reconnect-mode"
                    onChange={() => setConnectionDefault('reconnectMode', 'auto')}
                    type="radio"
                  />
                  <span className="advanced-toggle-name">{t.autoReconnect}</span>
                </label>
                <p className="advanced-toggle-hint">{t.autoReconnectHint}</p>
              </div>
            </div>
          </div>
        </fieldset>
        {connectionDefaultsError ? <p className="modal-error">{connectionDefaultsError}</p> : null}
      </section>
    </div>
  )
}
