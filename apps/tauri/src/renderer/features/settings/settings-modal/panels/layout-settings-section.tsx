import './layout-settings-section.css'
import type { FileTermDesktopApi, WorkspaceMode } from '@fileterm/core'
import { type LocaleMessages } from '../../../../i18n'
import { RadioCardGroup } from '../../../common/radio-card-group'
import { SelectionControl } from '../../../common/selection-control'
import { useSettingsModalContext } from '../context'

type LayoutSettingsSectionContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  workspaceMode: WorkspaceMode
  isSavingWorkspaceMode: boolean
  workspaceModeError: string | null
  setWorkspaceModePreference(nextValue: WorkspaceMode): void
  compactTabBar: boolean
  isSavingCompactTabBar: boolean
  compactTabBarError: string | null
  setCompactTabBarPreference(nextValue: boolean): void
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

export function LayoutSettingsSection() {
  const {
    t,
    desktopApi,
    workspaceMode,
    isSavingWorkspaceMode,
    workspaceModeError,
    setWorkspaceModePreference,
    compactTabBar,
    isSavingCompactTabBar,
    compactTabBarError,
    setCompactTabBarPreference
  } = useSettingsModalContext<LayoutSettingsSectionContext>()

  return (
    <section className="settings-section">
      <h3>{t.workspaceMode}</h3>
      <p className="settings-tools-hint">{t.workspaceModeHint}</p>
      <RadioCardGroup
        ariaLabel={t.workspaceMode}
        className="workspace-mode-picker"
        copyClassName="tab-bar-layout-copy"
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
      {workspaceModeError ? (
        <p className="modal-error" role="alert">
          {workspaceModeError}
        </p>
      ) : null}
      <div className="overview-preference-list">
        <label aria-busy={isSavingCompactTabBar} className="overview-preference-row" inert={isSavingCompactTabBar}>
          <span className="overview-preference-copy">
            <strong>{t.compactTabBar}</strong>
            <p>{t.compactTabBarHint}</p>
          </span>
          <span className="command-toggle overview-preference-toggle">
            <SelectionControl
              checked={compactTabBar}
              disabled={!desktopApi}
              onChange={(event) => setCompactTabBarPreference(event.target.checked)}
              type="checkbox"
            />
          </span>
        </label>
      </div>
      {compactTabBarError ? (
        <p className="modal-error" role="alert">
          {compactTabBarError}
        </p>
      ) : null}
    </section>
  )
}
