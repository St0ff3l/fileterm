import { useRef } from 'react'
import type { SshConnectionDefaults, TransferTask, UiPreferences } from '@fileterm/core'
import { t } from '../i18n'

export function useLatestRef<T>(value: T) {
  const ref = useRef(value)
  ref.current = value
  return ref
}

export type SyncedUiPreferences = Pick<
  UiPreferences,
  | 'theme'
  | 'themeConfig'
  | 'customThemes'
  | 'locale'
  | 'connectionDefaults'
  | 'terminalZoomLocked'
  | 'filePanelRememberRatio'
  | 'resourceMonitoringMetrics'
  | 'resourceMonitoringMetricOrder'
  | 'overviewShowStats'
  | 'overviewShowRecent'
  | 'overviewShowAllConnections'
  | 'overviewShowQuickActions'
  | 'overviewSectionOrder'
>

export function sameSyncedUiPreferences(left: SyncedUiPreferences, right: SyncedUiPreferences) {
  return (
    left.theme === right.theme &&
    JSON.stringify(left.themeConfig) === JSON.stringify(right.themeConfig) &&
    JSON.stringify(left.customThemes) === JSON.stringify(right.customThemes) &&
    left.locale === right.locale &&
    left.terminalZoomLocked === right.terminalZoomLocked &&
    left.filePanelRememberRatio === right.filePanelRememberRatio &&
    JSON.stringify(left.resourceMonitoringMetrics) === JSON.stringify(right.resourceMonitoringMetrics) &&
    JSON.stringify(left.resourceMonitoringMetricOrder) === JSON.stringify(right.resourceMonitoringMetricOrder) &&
    left.connectionDefaults.useEmptyPassword === right.connectionDefaults.useEmptyPassword &&
    left.connectionDefaults.enableExecChannel === right.connectionDefaults.enableExecChannel &&
    left.connectionDefaults.enableResourceMonitoring === right.connectionDefaults.enableResourceMonitoring &&
    left.connectionDefaults.resourceMonitoringIntervalSeconds ===
      right.connectionDefaults.resourceMonitoringIntervalSeconds &&
    left.connectionDefaults.reconnectMode === right.connectionDefaults.reconnectMode &&
    left.connectionDefaults.legacyAlgorithms === right.connectionDefaults.legacyAlgorithms &&
    left.overviewShowStats === right.overviewShowStats &&
    left.overviewShowRecent === right.overviewShowRecent &&
    left.overviewShowAllConnections === right.overviewShowAllConnections &&
    left.overviewShowQuickActions === right.overviewShowQuickActions &&
    left.overviewSectionOrder.length === right.overviewSectionOrder.length &&
    left.overviewSectionOrder.every((sectionId, index) => sectionId === right.overviewSectionOrder[index])
  )
}

export function syncedUiPreferencesFrom(preferences: UiPreferences): SyncedUiPreferences {
  return {
    theme: preferences.theme,
    themeConfig: preferences.themeConfig,
    customThemes: preferences.customThemes,
    locale: preferences.locale,
    connectionDefaults: { ...preferences.connectionDefaults },
    terminalZoomLocked: preferences.terminalZoomLocked,
    filePanelRememberRatio: preferences.filePanelRememberRatio,
    resourceMonitoringMetrics: [...preferences.resourceMonitoringMetrics],
    resourceMonitoringMetricOrder: [...preferences.resourceMonitoringMetricOrder],
    overviewShowStats: preferences.overviewShowStats,
    overviewShowRecent: preferences.overviewShowRecent,
    overviewShowAllConnections: preferences.overviewShowAllConnections,
    overviewShowQuickActions: preferences.overviewShowQuickActions,
    overviewSectionOrder: preferences.overviewSectionOrder
  }
}

function isUploadPermissionFailure(transfer: TransferTask) {
  if (transfer.direction !== 'upload' || !['failed', 'paused', 'interrupted'].includes(transfer.status)) {
    return false
  }

  return /permission[\s_-]*denied|access[\s_-]*denied|operation[\s_-]*not[\s_-]*permitted|not[\s_-]*permitted|authorization[\s_-]*failed|\b(?:eacces|eperm)\b|权限不足|没有权限|无权|拒绝访问/i.test(
    transfer.message ?? ''
  )
}

function isRootUploadCommandFailure(transfer: TransferTask) {
  if (transfer.direction !== 'upload' || !['failed', 'paused', 'interrupted'].includes(transfer.status)) {
    return false
  }

  return /root\s+(?:文件|上传|写入)|(?:^|\b)(?:su|sudo)\s+|密码|password/i.test(transfer.message ?? '')
}

export function uploadFailureBanner(transfer: TransferTask) {
  if (isUploadPermissionFailure(transfer)) {
    return t.uploadPermissionDenied
  }
  if (isRootUploadCommandFailure(transfer)) {
    const detail = transfer.message?.replace(/^command error:\s*/i, '').trim()
    return detail ? `${t.uploadFailed}: ${detail}` : t.uploadFailed
  }
  return null
}

export type { SshConnectionDefaults }
