import './connections-settings-panel.css'
import { useEffect, useState } from 'react'
import type { FileTermDesktopApi, SshConnectionDefaults } from '@fileterm/core'
import { DropdownSelect } from '../../../common/dropdown-select'
import { ResourceMonitoringMetricsEditor } from '../../../common/resource-monitoring-metrics-editor'
import { SelectionControl } from '../../../common/selection-control'
import { TransferSidebarSettingControl } from '../../../../components/common/transfer-sidebar-setting-control'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type ConnectionsSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  connectionDefaults: SshConnectionDefaults
  isSavingConnectionDefaults: boolean
  setConnectionDefault<K extends keyof SshConnectionDefaults>(key: K, value: SshConnectionDefaults[K]): void
  connectionDefaultsError: string | null
  transferSidebarCollapsedByDefault: boolean
  isSavingTransferSidebarPreference: boolean
  setTransferSidebarCollapsedByDefaultPreference(value: boolean): void
  transferSidebarPreferenceError: string | null
}

export function ConnectionsSettingsPanel() {
  const {
    t,
    desktopApi,
    connectionDefaults,
    isSavingConnectionDefaults,
    setConnectionDefault,
    connectionDefaultsError,
    transferSidebarCollapsedByDefault,
    isSavingTransferSidebarPreference,
    setTransferSidebarCollapsedByDefaultPreference,
    transferSidebarPreferenceError
  } = useSettingsModalContext<ConnectionsSettingsPanelContext>()
  // Commit the socket path on blur/Enter instead of saving every keystroke.
  const [agentSocketDraft, setAgentSocketDraft] = useState(connectionDefaults.sshAgentSocketPath)
  useEffect(() => setAgentSocketDraft(connectionDefaults.sshAgentSocketPath), [connectionDefaults.sshAgentSocketPath])
  const commitAgentSocket = () => {
    const next = agentSocketDraft.trim()
    if (next !== connectionDefaults.sshAgentSocketPath) setConnectionDefault('sshAgentSocketPath', next)
  }

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.connectionDefaults}</h3>
        <p className="settings-tools-hint">{t.connectionDefaultsHint}</p>
        <fieldset
          className="settings-connection-defaults"
          disabled={!desktopApi}
          inert={isSavingConnectionDefaults || isSavingTransferSidebarPreference}
          aria-busy={isSavingConnectionDefaults || isSavingTransferSidebarPreference}
          style={{ border: 0, margin: 0, padding: 0 }}
        >
          <div className="settings-connection-defaults__group">
            <h4>{t.advancedSettings}</h4>
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
                  disabled={!connectionDefaults.enableResourceMonitoring}
                  onMetricsChange={(next) => setConnectionDefault('resourceMonitoringMetrics', next)}
                  onOrderChange={(next) => setConnectionDefault('resourceMonitoringMetricOrder', next)}
                />
              </div>
              <TransferSidebarSettingControl
                checked={transferSidebarCollapsedByDefault}
                disabled={!desktopApi || isSavingTransferSidebarPreference}
                hint={t.transferSidebarCollapsedByDefaultHint}
                label={t.transferSidebarCollapsedByDefault}
                onChange={setTransferSidebarCollapsedByDefaultPreference}
              />
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
          </div>
          <div className="settings-connection-defaults__group">
            <h4>{t.sshAgentSettings}</h4>
            <label className="settings-connection-defaults__field">
              <span>{t.sshAgentSocketPath}</span>
              <input
                spellCheck={false}
                autoCapitalize="off"
                placeholder={t.sshAgentSocketPathPlaceholder}
                value={agentSocketDraft}
                onChange={(event) => setAgentSocketDraft(event.target.value)}
                onBlur={commitAgentSocket}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') commitAgentSocket()
                }}
              />
            </label>
            <p className="advanced-toggle-hint">{t.sshAgentSocketPathHint}</p>
          </div>
          <div className="settings-connection-defaults__group">
            <h4>{t.reliability}</h4>
            <div className="reconnect-mode-group" style={{ marginTop: 0 }}>
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
          </div>
        </fieldset>
        {connectionDefaultsError ? <p className="modal-error">{connectionDefaultsError}</p> : null}
        {transferSidebarPreferenceError ? <p className="modal-error">{transferSidebarPreferenceError}</p> : null}
      </section>
    </div>
  )
}
