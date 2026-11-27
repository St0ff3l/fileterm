import './workspace-mode-settings-panel.css'
import { useId } from 'react'
import {
  TOPOLOGY_SERVICE_PROBE_INTERVAL_OPTIONS,
  type FileTermDesktopApi,
  type TopologyServiceProbeIntervalSeconds,
  type WorkspaceMode
} from '@fileterm/core'
import { DropdownSelect } from '../../../../components/common/dropdown-select/dropdown-select'
import { FeedbackBubble } from '../../../../components/common/feedback-bubble/feedback-bubble'
import { type LocaleMessages } from '../../../../i18n'
import { SelectionControl } from '../../../common/selection-control'
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

/** A miniature window schematic illustrating each workspace mode's real chrome and pane order. */
function WorkspaceModePreview({ mode }: { mode: WorkspaceMode }) {
  const tabCount = mode === 'server-management' ? 2 : 4

  return (
    <span aria-hidden="true" className={`workspace-mode-preview is-${mode}`}>
      <span className="workspace-mode-preview-brand">
        <span className="workspace-mode-preview-dots">
          <span />
          <span />
          <span />
        </span>
      </span>
      <span className="workspace-mode-preview-tabs">
        {Array.from({ length: tabCount }, (_, index) => (
          <span key={index} />
        ))}
      </span>
      {mode === 'server-management' ? (
        <span className="workspace-mode-preview-nav">
          <span className="workspace-mode-preview-nav-item is-active" />
          <span className="workspace-mode-preview-nav-item" />
          <span className="workspace-mode-preview-nav-item" />
          <span className="workspace-mode-preview-nav-divider" />
          <span className="workspace-mode-preview-nav-host">
            <span className="workspace-mode-preview-host-dot" />
            <span className="workspace-mode-preview-host-bar" />
          </span>
          <span className="workspace-mode-preview-nav-host">
            <span className="workspace-mode-preview-host-dot is-idle" />
            <span className="workspace-mode-preview-host-bar is-short" />
          </span>
        </span>
      ) : null}
      <span className="workspace-mode-preview-info">
        <span className="workspace-mode-preview-info-block" />
        <span className="workspace-mode-preview-info-block is-tall" />
      </span>
      <span className="workspace-mode-preview-terminal">
        <span className="workspace-mode-preview-line is-prompt" />
        <span className="workspace-mode-preview-line is-long" />
        <span className="workspace-mode-preview-line is-medium" />
        <span className="workspace-mode-preview-line is-short" />
      </span>
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
  const radioGroupId = useId()
  const modeDisabled = !desktopApi || isSavingWorkspaceMode
  const modeOptions: ReadonlyArray<{ value: WorkspaceMode; title: string; description: string }> = [
    {
      value: 'standard',
      title: t.workspaceModeStandard,
      description: t.workspaceModeStandardHint
    },
    {
      value: 'server-management',
      title: t.workspaceModeServerManagement,
      description: t.workspaceModeServerManagementHint
    }
  ]

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.workspaceMode}</h3>
        <p className="settings-tools-hint">{t.workspaceModeHint}</p>
        <div aria-label={t.workspaceMode} className="workspace-mode-picker" role="radiogroup">
          {modeOptions.map((option) => {
            const selected = workspaceMode === option.value
            return (
              <label
                key={option.value}
                className={`workspace-mode-card ${selected ? 'is-selected' : ''} ${modeDisabled ? 'is-disabled' : ''}`.trim()}
              >
                <span className="workspace-mode-card__stage">
                  <WorkspaceModePreview mode={option.value} />
                </span>
                <span className="workspace-mode-card__body">
                  <span className="workspace-mode-card__header">
                    <span className="workspace-mode-card__title">{option.title}</span>
                    <SelectionControl
                      checked={selected}
                      className="workspace-mode-card__radio"
                      disabled={modeDisabled}
                      name={`workspace-mode-${radioGroupId}`}
                      onChange={() => setWorkspaceModePreference(option.value)}
                      type="radio"
                      value={option.value}
                    />
                  </span>
                  <span className="workspace-mode-card__description">{option.description}</span>
                </span>
              </label>
            )
          })}
        </div>
        <FeedbackBubble message={workspaceModeError} sourceKey="settings.workspace-mode" tone="error" />
        {workspaceMode === 'server-management' ? (
          <>
            <div aria-busy={isSavingTopologyServiceProbeInterval} className="workspace-mode-probe-setting">
              <div className="workspace-mode-probe-setting-copy">
                <strong>{t.topologyServiceProbeInterval}</strong>
                <p>{t.topologyServiceProbeIntervalHint}</p>
              </div>
              <div className="workspace-mode-probe-setting-control">
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
            </div>
            <FeedbackBubble
              message={topologyServiceProbeIntervalError}
              sourceKey="settings.topology-service-probe-interval"
              tone="error"
            />
          </>
        ) : null}
      </section>
      {workspaceMode === 'standard' ? <OverviewContentSettingsSection /> : null}
    </div>
  )
}
