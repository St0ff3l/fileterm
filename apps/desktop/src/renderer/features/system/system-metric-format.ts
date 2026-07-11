import type { SystemMetrics } from '@fileterm/core'

interface SystemLoadLabels {
  busyLogicalProcessorsUnit: string
  busyLogicalProcessorsDescription: string
}

export function formatSystemLoad(
  metrics: Pick<SystemMetrics, 'load' | 'loadUnit'> | undefined,
  labels: SystemLoadLabels
) {
  if (!metrics?.load || metrics.load === '-') {
    return { value: '-', title: '' }
  }
  if (metrics.loadUnit === 'busy-logical-processors') {
    return {
      value: `${metrics.load}${labels.busyLogicalProcessorsUnit}`,
      title: labels.busyLogicalProcessorsDescription
    }
  }
  return { value: metrics.load, title: '' }
}
