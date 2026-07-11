import { busyboxCollector } from './busybox-collector.js'
import { linuxCollector } from './linux-collector.js'
import { probeRemoteSystemPlatform } from './platform-probe.js'
import type {
  RemoteSystemPlatform,
  SystemMetricsCollectionResult,
  SystemMetricsCollector,
  SystemMetricsExecutor
} from './types.js'
import { windowsCollector } from './windows-metrics-collector.js'

const collectors: Record<Exclude<RemoteSystemPlatform, 'unknown'>, SystemMetricsCollector> = {
  linux: linuxCollector,
  busybox: busyboxCollector,
  windows: windowsCollector
}

export async function collectSshSystemMetrics(
  executor: SystemMetricsExecutor,
  preferredPlatform?: RemoteSystemPlatform
): Promise<SystemMetricsCollectionResult> {
  if (preferredPlatform && preferredPlatform !== 'unknown') {
    const collector = collectors[preferredPlatform]
    const metrics = await collector.collect(executor)
    return { platform: metrics.platform ?? preferredPlatform, metrics }
  }

  const platform = await probeRemoteSystemPlatform(executor)
  if (platform === 'unknown') {
    throw new Error('Unable to determine remote system platform')
  }
  const collector = collectors[platform]
  const metrics = await collector.collect(executor)
  return {
    platform: metrics.platform ?? collector.platform,
    metrics
  }
}

export type { RemoteSystemPlatform, SystemMetricsExecutor } from './types.js'
