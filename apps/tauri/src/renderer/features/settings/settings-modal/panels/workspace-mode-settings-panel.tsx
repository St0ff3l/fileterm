import './workspace-mode-settings-panel.css'
import {
  TOPOLOGY_SERVICE_PROBE_INTERVAL_OPTIONS,
  type FileTermDesktopApi,
  type TopologyServiceProbeIntervalSeconds,
  type WorkspaceMode
} from '@fileterm/core'
import { DropdownSelect } from '../../../../components/common/dropdown-select/dropdown-select'
import { FeedbackBubble } from '../../../../components/common/feedback-bubble/feedback-bubble'
import { type LocaleMessages } from '../../../../i18n'
import { RadioCardGroup } from '../../../common/radio-card-group'
import { useSettingsModalContext } from '../context'
import { OverviewContentSettingsSection } from './overview-content-settings-section'

type WorkspaceModeSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  workspaceMode: WorkspaceMode
  isSavingWorkspaceMode: boolean
  workspaceModeError: string | null
  setWorkspaceModePreference(nextValue: WorkspaceMode): void
  topologyServiceProbeIntervalSeconds: TopologyServiceProbeIntervalSeconds
  isSavingTopologyServiceProbeInterval: boolean
  topologyServiceProbeIntervalError: string | null
  setTopologyServiceProbeIntervalPreference(nextValue: TopologyServiceProbeIntervalSeconds): void
}

/** A miniature window: tab row on top, then the panes in their real order. */
function WorkspaceModePreview({ mode }: { mode: WorkspaceMode }) {
  const tabCount = mode === 'server-management' ? 2 : 4

  return (
    <span aria-hidden="true" className={`workspace-mode-preview is-${mode}`}>
      <span className="workspace-mode-preview-tabs">
        {Array.from({ length: tabCount }, (_, index) => (
          <span key={index} />
        ))}
      </span>
      {mode === 'server-management' ? (
        <span className="workspace-mode-preview-nav">
          <span />
          <span />
          <span />
        </span>
      ) : null}
      <span className="workspace-mode-preview-info" />
      <span className="workspace-mode-preview-terminal" />
    </span>
  )
}

export function WorkspaceModeSettingsPanel() {
  const {
    t,
    desktopApi,
    workspaceMode,
    isSavingWorkspaceMode,
    workspaceModeError,
    setWorkspaceModePreference,
    topologyServiceProbeIntervalSeconds,
    isSavingTopologyServiceProbeInterval,
    topologyServiceProbeIntervalError,
    setTopologyServiceProbeIntervalPreference
  } = useSettingsModalContext<WorkspaceModeSettingsPanelContext>()

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.workspaceMode}</h3>
        <p className="settings-tools-hint">{t.workspaceModeHint}</p>
        <RadioCardGroup
          ariaLabel={t.workspaceMode}
          className="workspace-mode-picker"
          copyClassName="workspace-mode-copy"
          disabled={!desktopApi || isSavingWorkspaceMode}
          name="workspace-mode"
          onChange={setWorkspaceModePreference}
          options={[
            {
              value: 'standard',
              label: (
                <>
                  <WorkspaceModePreview mode="standard" />
                  <span>{t.workspaceModeStandard}</span>
                </>
              ),
              description: t.workspaceModeStandardHint
            },
            {
              value: 'server-management',
              label: (
                <>
                  <WorkspaceModePreview mode="server-management" />
                  <span>{t.workspaceModeServerManagement}</span>
                </>
              ),
              description: t.workspaceModeServerManagementHint
            }
          ]}
          value={workspaceMode}
        />
        <FeedbackBubble message={workspaceModeError} sourceKey="settings.workspace-mode" tone="error" />
        <div aria-busy={isSavingTopologyServiceProbeInterval} className="workspace-mode-probe-setting">
          <div className="workspace-mode-probe-setting-copy">
            <strong>{t.topologyServiceProbeInterval}</strong>
            <p>{t.topologyServiceProbeIntervalHint}</p>
          </div>
          <DropdownSelect
            ariaLabel={t.topologyServiceProbeInterval}
            disabled={!desktopApi || isSavingTopologyServiceProbeInterval}
            onChange={(value) =>
              setTopologyServiceProbeIntervalPreference(Number(value) as TopologyServiceProbeIntervalSeconds)
            }
            options={TOPOLOGY_SERVICE_PROBE_INTERVAL_OPTIONS.map((seconds) => ({
              value: String(seconds),
              label: t.topologyServiceProbeIntervalOption.replace('{seconds}', String(seconds))
            }))}
            value={String(topologyServiceProbeIntervalSeconds)}
          />
        </div>
        <FeedbackBubble
          message={topologyServiceProbeIntervalError}
          sourceKey="settings.topology-service-probe-interval"
          tone="error"
        />
      </section>
      {workspaceMode === 'standard' ? <OverviewContentSettingsSection /> : null}
    </div>
  )
}
