import type { Dispatch, SetStateAction } from 'react'
import type { FilePaneMode, RemoteFileItem, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StableButtonLabel } from '../common/stable-button-content'
import { DropdownSelect } from '../common/dropdown-select'
import './file-manager-toolbar.css'

export function FileManagerToolbar({
  leftPaneMode,
  leftPaneModeError,
  onLeftPaneModeChange,
  activeSession,
  activeTab,
  activeView,
  canManageTunnels,
  canUseRemoteFiles,
  clipboardStatusText,
  isSshSession,
  onChooseUploadFiles,
  onDownloadFiles,
  onOpenCommandManager,
  onOpenFileTransfers,
  onRefresh,
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  selectedRemoteDownloadItems,
  setResetColumnsTrigger,
  switchActiveView
}: {
  leftPaneMode: FilePaneMode
  leftPaneModeError: string | null
  onLeftPaneModeChange(mode: FilePaneMode): void
  activeSession: SessionSnapshot
  activeTab: WorkspaceTab | null
  activeView: 'file' | 'command' | 'tunnel'
  canManageTunnels: boolean
  canUseRemoteFiles: boolean
  clipboardStatusText: string | null
  isSshSession: boolean
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[]): void
  onOpenCommandManager(): void
  onOpenFileTransfers(): void
  onRefresh(): void
  onToggleRemoteFileAccessMode(): void
  remoteFileAccessMode: 'user' | 'root'
  selectedRemoteDownloadItems: RemoteFileItem[]
  setResetColumnsTrigger: Dispatch<SetStateAction<number>>
  switchActiveView(nextView: 'file' | 'command' | 'tunnel'): void
}) {
  return (
    <div className="file-tabs">
      <div className="file-tabs-left">
        <button
          className={activeView === 'file' ? 'active' : ''}
          type="button"
          onClick={() => switchActiveView('file')}
        >
          {t.file}
        </button>
        {isSshSession ? (
          <button
            className={activeView === 'command' ? 'active' : ''}
            type="button"
            onClick={() => switchActiveView('command')}
          >
            {t.command}
          </button>
        ) : null}
        {canManageTunnels ? (
          <button
            className={activeView === 'tunnel' ? 'active' : ''}
            type="button"
            onClick={() => switchActiveView('tunnel')}
          >
            {t.tunnel}
          </button>
        ) : null}
      </div>
      <span
        className={`file-current-path ${activeView !== 'file' ? 'file-pane-view-label' : clipboardStatusText ? 'is-status-hint' : ''}`}
      >
        {activeView === 'file'
          ? clipboardStatusText || activeSession.remotePath
          : activeView === 'command'
            ? t.commandPreview
            : t.runtimeTunnelTab}
      </span>
      {activeView === 'file' ? (
        <div className="file-tab-actions">
          <button
            className="file-tab-icon-action"
            title={t.refresh}
            type="button"
            disabled={!canUseRemoteFiles}
            onClick={() => {
              onRefresh()
              setResetColumnsTrigger((prev) => prev + 1)
            }}
          >
            <AppIcon name="refresh" />
          </button>
          <button
            className="file-transfer-launch"
            type="button"
            title={t.fileTransferWorkspace}
            onClick={onOpenFileTransfers}
          >
            <AppIcon name="transfer" />
            {t.fileTransferWorkspace}
          </button>
          <DropdownSelect
            className="file-pane-mode-select"
            forceCustomMenu
            ariaLabel={t.filePaneDisplayMode}
            value={leftPaneMode}
            options={[
              { value: 'local', label: t.filePaneLocal },
              { value: 'remote-tree', label: t.remoteDirectoryTree }
            ]}
            onChange={(value) => onLeftPaneModeChange(value === 'remote-tree' ? 'remote-tree' : 'local')}
          />
          {leftPaneModeError ? (
            <span role="status" className="file-pane-mode-error" title={leftPaneModeError}>
              {t.filePaneModeError}
            </span>
          ) : null}
          {activeTab?.sessionType === 'ssh' ? (
            <button
              aria-pressed={remoteFileAccessMode === 'root'}
              className={remoteFileAccessMode === 'root' ? 'active' : ''}
              disabled={!canUseRemoteFiles}
              title={`${remoteFileAccessMode === 'root' ? t.fileRootView : t.fileUserView} - ${t.fileRootViewHint}`}
              type="button"
              onClick={onToggleRemoteFileAccessMode}
            >
              <StableButtonLabel
                label={remoteFileAccessMode === 'root' ? activeSession.sudoUser || 'root' : 'user'}
                reserveLabel={remoteFileAccessMode === 'root' ? 'user' : activeSession.sudoUser || 'root'}
              />
            </button>
          ) : null}
          <button
            className="file-tab-icon-action"
            title={t.downloadTo}
            type="button"
            disabled={!canUseRemoteFiles || !selectedRemoteDownloadItems.length}
            onClick={() => onDownloadFiles(selectedRemoteDownloadItems)}
          >
            <AppIcon name="download" />
          </button>
          <button
            className="file-tab-icon-action"
            title={t.upload}
            type="button"
            disabled={!canUseRemoteFiles}
            onClick={onChooseUploadFiles}
          >
            <AppIcon name="upload" />
          </button>
        </div>
      ) : activeView === 'command' ? (
        <div className="file-tab-actions file-tab-actions-command">
          <button className="flat-button compact command-manager-launch" type="button" onClick={onOpenCommandManager}>
            {t.commandManager}
          </button>
        </div>
      ) : null}
    </div>
  )
}
