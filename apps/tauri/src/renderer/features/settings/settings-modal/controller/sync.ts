import { useEffect } from 'react'
import { waitForMinimumBusyDuration } from '../../../common/operation-timing'
import type { SettingsModalState } from './state'

type SyncOperation = Exclude<SettingsModalState['syncOperation'], 'load' | null>

export function useSyncSettingsController({
  state,
  openSecuritySettings
}: {
  state: SettingsModalState
  openSecuritySettings(focusBackupPassword?: boolean): void
}) {
  const {
    activeTab,
    desktopApi,
    syncConfig,
    setSyncConfig,
    syncPassword,
    setSyncPassword,
    syncFeedback,
    setSyncFeedback,
    syncSubTab,
    setSyncSubTab,
    syncOperation,
    setSyncOperation,
    syncOperationRef,
    s3Config,
    setS3Config,
    s3SecretAccessKey,
    setS3SecretAccessKey,
    s3Feedback,
    setS3Feedback,
    backupUploadMode,
    setBackupUploadMode,
    backupDownloadMode,
    setBackupDownloadMode
  } = state

  useEffect(() => {
    if (activeTab !== 'sync' || !desktopApi) return
    if (syncOperationRef.current) return
    setSyncFeedback(null)
    setS3Feedback(null)
    syncOperationRef.current = 'load'
    setSyncOperation('load')
    void desktopApi
      .getWebDavSyncConfig()
      .then(async (webDavConfig) => {
        setSyncConfig(webDavConfig)
        setS3Config(await desktopApi.getS3BackupConfig())
      })
      .catch((error: unknown) =>
        setSyncFeedback({ kind: 'error', message: error instanceof Error ? error.message : String(error) })
      )
      .finally(() => {
        if (syncOperationRef.current === 'load') {
          syncOperationRef.current = null
          setSyncOperation(null)
        }
      })
  }, [activeTab, desktopApi])

  const runSyncOperation = async (operation: SyncOperation, action: () => Promise<void>) => {
    if (syncOperationRef.current) return
    const operationStartedAt = performance.now()
    syncOperationRef.current = operation
    setSyncOperation(operation)
    if (operation.startsWith('s3-')) {
      setS3Feedback(null)
    } else {
      setSyncFeedback(null)
    }
    try {
      await action()
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (message.includes('SECURITY_BACKUP_PASSWORD_REQUIRED')) {
        openSecuritySettings(true)
      } else if (operation.startsWith('s3-')) {
        setS3Feedback({ kind: 'error', message })
      } else {
        setSyncFeedback({ kind: 'error', message })
      }
    } finally {
      await waitForMinimumBusyDuration(operationStartedAt)
      if (syncOperationRef.current === operation) {
        syncOperationRef.current = null
        setSyncOperation(null)
      }
    }
  }

  return {
    syncConfig,
    setSyncConfig,
    syncPassword,
    setSyncPassword,
    syncFeedback,
    setSyncFeedback,
    syncSubTab,
    setSyncSubTab,
    syncOperation,
    runSyncOperation,
    s3Config,
    setS3Config,
    s3SecretAccessKey,
    setS3SecretAccessKey,
    s3Feedback,
    setS3Feedback,
    backupUploadMode,
    setBackupUploadMode,
    backupDownloadMode,
    setBackupDownloadMode
  }
}
