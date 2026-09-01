import type { AppUpdateStatus, FileTermDesktopApi, UiPreferences } from '@fileterm/core'
import { useState } from 'react'
import { AppIcon } from '../../../common/app-icon'
import { DropdownSelect } from '../../../common/dropdown-select'
import { PortableUpdateDialog } from '../../../common/portable-update-dialog'
import { StableButtonContent, StableButtonLabel } from '../../../common/stable-button-content'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'

type UpdatesSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  updateStatus: AppUpdateStatus | null
  autoCheckUpdates: boolean
  isSavingUpdatePreference: boolean
  updatePreferenceError: string | null
  setUpdateCheckPreference(nextValue: boolean): void
  updateChannel: UiPreferences['updateChannel']
  setUpdateChannelPreference(nextValue: UiPreferences['updateChannel']): void
}

export function UpdatesSettingsPanel() {
  const {
    t,
    desktopApi,
    updateStatus,
    autoCheckUpdates,
    isSavingUpdatePreference,
    updatePreferenceError,
    setUpdateCheckPreference,
    updateChannel,
    setUpdateChannelPreference
  } = useSettingsModalContext<UpdatesSettingsPanelContext>()
  const [isPortableUpdateDialogOpen, setPortableUpdateDialogOpen] = useState(false)

  return (
    <div className="settings-panel">
      <section className="settings-section">
        <h3>{t.appUpdates}</h3>
        <div className="update-check-preference">
          <div>
            <strong>{t.updateCheckPreference}</strong>
            <p>{t.updateCheckPreferenceHint}</p>
          </div>
          <DropdownSelect
            className="update-check-preference-select"
            disabled={!desktopApi || isSavingUpdatePreference}
            onChange={(value) => setUpdateCheckPreference(value === 'auto')}
            value={autoCheckUpdates ? 'auto' : 'manual'}
            options={[
              { value: 'auto', label: t.autoCheckUpdates },
              { value: 'manual', label: t.doNotAutoUpdate }
            ]}
          />
        </div>
        <div className="update-check-preference">
          <div>
            <strong>{t.updateChannel}</strong>
            <p>{t.updateChannelHint}</p>
          </div>
          <DropdownSelect
            className="update-check-preference-select"
            disabled={!desktopApi || isSavingUpdatePreference}
            onChange={(value) => setUpdateChannelPreference(value === 'beta' ? 'beta' : 'stable')}
            value={updateChannel}
            options={[
              { value: 'stable', label: t.stableChannel },
              { value: 'beta', label: t.betaChannel }
            ]}
          />
        </div>
        {updatePreferenceError ? <p className="modal-error">{updatePreferenceError}</p> : null}
        <div className="update-status-card" aria-live="polite">
          <div>
            <strong>{t.updateStatus}</strong>
            <p>{getUpdateStatusLabel(updateStatus, t, autoCheckUpdates)}</p>
          </div>
          <span className={`update-status-indicator ${updateStatus?.state ?? 'idle'}`} />
        </div>
        {updateStatus?.state === 'downloading' ? (
          <div className="update-progress" aria-label={t.updateDownloading}>
            <span style={{ width: `${updateStatus.progress ?? 0}%` }} />
          </div>
        ) : null}
        <div className="settings-update-actions">
          {updateStatus?.state === 'available' ? (
            <button
              className="primary-button compact"
              onClick={() => {
                if (updateStatus.updateMode === 'release-page') {
                  if (updateStatus.isPortable) {
                    setPortableUpdateDialogOpen(true)
                  } else {
                    void desktopApi?.openExternalUrl(
                      updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases'
                    )
                  }
                } else {
                  void desktopApi?.downloadUpdate()
                }
              }}
              type="button"
            >
              <StableButtonLabel
                label={updateStatus.updateMode === 'release-page' ? t.openReleasePage : t.downloadUpdate}
                reserveLabel={updateStatus.updateMode === 'release-page' ? t.downloadUpdate : t.openReleasePage}
              />
            </button>
          ) : null}
          {updateStatus?.state === 'downloaded' ? (
            <button className="primary-button compact" onClick={() => void desktopApi?.installUpdate()} type="button">
              {t.restartToUpdate}
            </button>
          ) : null}
          {updateStatus?.state !== 'downloading' && updateStatus?.state !== 'downloaded' ? (
            <button
              aria-busy={updateStatus?.state === 'checking'}
              className="flat-button compact"
              disabled={updateStatus?.state === 'checking' || updateStatus?.state === 'unsupported'}
              onClick={() => void desktopApi?.checkForUpdates()}
              type="button"
            >
              <StableButtonContent
                busy={updateStatus?.state === 'checking'}
                busyLabel={t.checkingForUpdates}
                icon={<AppIcon name="refresh" size={14} />}
                label={t.checkForUpdates}
              />
            </button>
          ) : null}
        </div>
      </section>
      {isPortableUpdateDialogOpen ? (
        <PortableUpdateDialog
          onClose={() => setPortableUpdateDialogOpen(false)}
          onOpenReleasePage={() =>
            void desktopApi?.openExternalUrl(updateStatus?.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
          }
        />
      ) : null}
    </div>
  )
}

function getUpdateStatusLabel(status: AppUpdateStatus | null, labels: LocaleMessages, autoCheckUpdates: boolean) {
  if (!status) return autoCheckUpdates ? labels.updateStatusIdle : labels.updateStatusManual
  if (status.state === 'available') {
    const label = status.updateMode === 'release-page' ? labels.updateAvailableManual : labels.updateAvailable
    return label.replace('{version}', status.availableVersion ?? '—')
  }
  if (status.state === 'downloaded') return labels.updateDownloaded.replace('{version}', status.availableVersion ?? '—')
  if (status.state === 'downloading')
    return labels.updateDownloading.replace('{progress}', String(status.progress ?? 0))
  if (status.state === 'not-available') return labels.updateNotAvailable
  if (status.state === 'checking') return labels.checkingForUpdates
  if (status.state === 'error') return `${labels.updateFailed}: ${status.message ?? '—'}`
  if (status.state === 'unsupported') return labels.updateUnsupported
  return labels.updateStatusIdle
}
