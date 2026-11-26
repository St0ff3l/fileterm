import { useEffect, useState } from 'react'
import type { AppUpdateStatus } from '@fileterm/core'
import { MaterialIcon, type MaterialIconName } from '../material-icon/material-icon'
import { t } from '../../../i18n'
import { useExternalUrl } from '../../../hooks/use-external-url'
import { FeedbackText } from '../../../features/common/feedback-text'
import { PortableUpdateDialog } from '../../../features/common/portable-update-dialog'
import './sidebar-update-hint.css'

export function SidebarUpdateHint({ active, collapsed = false }: { active: boolean; collapsed?: boolean }) {
  const desktopApi = window.fileterm
  const { openExternalUrl, externalUrlError } = useExternalUrl(desktopApi, t.openExternalUrlFailed)
  const updatePreviewState = import.meta.env.DEV ? import.meta.env.VITE_UPDATE_PREVIEW : undefined
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null)
  const [isPortableUpdateDialogOpen, setPortableUpdateDialogOpen] = useState(false)

  useEffect(() => {
    if (!active) return
    if (updatePreviewState) {
      setUpdateStatus({
        currentVersion: desktopApi?.appVersion ?? '1.0.0',
        state:
          updatePreviewState === 'downloading' || updatePreviewState === 'downloaded' || updatePreviewState === 'error'
            ? updatePreviewState
            : 'available',
        availableVersion: '1.1.0',
        progress: updatePreviewState === 'downloading' ? 62 : updatePreviewState === 'downloaded' ? 100 : undefined,
        message: updatePreviewState === 'error' ? t.updateServerUnavailable : undefined
      })
      return
    }
    if (!desktopApi) return
    void desktopApi.getUpdateStatus().then(setUpdateStatus)
    return desktopApi.onUpdateStatus(setUpdateStatus)
  }, [active, desktopApi, updatePreviewState])

  const updateAction = () => {
    if (updateStatus?.state === 'available') {
      if (updateStatus.updateMode === 'release-page') {
        if (updateStatus.isPortable) {
          setPortableUpdateDialogOpen(true)
        } else {
          void openExternalUrl(updateStatus.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
        }
      } else {
        void desktopApi?.downloadUpdate()
      }
    } else if (updateStatus?.state === 'downloaded') {
      void desktopApi?.installUpdate()
    } else if (updateStatus?.state === 'error') {
      void desktopApi?.checkForUpdates()
    }
  }

  const updateHint: { icon: MaterialIconName; label: string; title: string } | null =
    updateStatus?.state === 'available'
      ? {
          icon: 'system_update_alt',
          label: t.updateAvailableShort.replace('{version}', updateStatus.availableVersion ?? '—'),
          title: updateStatus.updateMode === 'release-page' ? t.openReleasePage : t.downloadUpdate
        }
      : updateStatus?.state === 'downloading'
        ? {
            icon: 'downloading',
            label: t.updateDownloadingShort.replace('{progress}', String(updateStatus.progress ?? 0)),
            title: t.updateDownloading.replace('{progress}', String(updateStatus.progress ?? 0))
          }
        : updateStatus?.state === 'downloaded'
          ? { icon: 'restart_alt', label: t.restartToUpdate, title: t.restartToUpdate }
          : updateStatus?.state === 'error'
            ? { icon: 'error', label: t.updateRetry, title: updateStatus.message ?? t.updateFailed }
            : null

  if (!active) return null

  return (
    <div className={`sidebar-update-control ${collapsed ? 'is-collapsed' : ''}`}>
      {updateHint ? (
        <button
          aria-label={updateHint.title}
          className={`sidebar-update-hint is-${updateStatus?.state}`}
          disabled={updateStatus?.state === 'downloading'}
          onClick={updateAction}
          title={updateHint.title}
          type="button"
        >
          <MaterialIcon name={updateHint.icon} aria-hidden="true" className="material-symbols-outlined" />
          <span>{updateHint.label}</span>
        </button>
      ) : null}
      <FeedbackText message={externalUrlError} tone="error" />
      {isPortableUpdateDialogOpen ? (
        <PortableUpdateDialog
          onClose={() => setPortableUpdateDialogOpen(false)}
          onOpenReleasePage={() => {
            if (import.meta.env.DEV && !updateStatus?.isPortable) return
            void openExternalUrl(updateStatus?.releaseUrl ?? 'https://github.com/St0ff3l/fileterm/releases')
          }}
        />
      ) : null}
    </div>
  )
}
