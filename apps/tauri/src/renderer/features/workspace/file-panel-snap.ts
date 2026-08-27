export const FILE_PANEL_SNAP_TARGETS = ['disk-header', 'system-title', 'process-tabs', 'network-panel'] as const

export type FilePanelSnapTarget = (typeof FILE_PANEL_SNAP_TARGETS)[number]

export const DEFAULT_FILE_PANEL_SNAP_TARGET: FilePanelSnapTarget = 'disk-header'

export const FILE_PANEL_SNAP_TARGET_SELECTORS: Record<FilePanelSnapTarget, string> = {
  'disk-header': '[data-file-panel-snap-target="disk-header"]',
  'system-title': '[data-file-panel-snap-target="system-title"]',
  'process-tabs': '[data-file-panel-snap-target="process-tabs"]',
  'network-panel': '[data-file-panel-snap-target="network-panel"]'
}

export function isFilePanelSnapTarget(value: unknown): value is FilePanelSnapTarget {
  return typeof value === 'string' && FILE_PANEL_SNAP_TARGETS.includes(value as FilePanelSnapTarget)
}
