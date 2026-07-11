import type { RemoteSystemPlatform, SystemMetrics } from '@fileterm/core'

export type { RemoteSystemPlatform }

export interface SystemMetricsCommandOptions {
  allowNonZeroWithStdout?: boolean
  timeoutMs?: number
}

export interface SystemMetricsExecutor {
  exec(command: string, options?: SystemMetricsCommandOptions, stdinPayload?: string): Promise<string>
}

export interface SystemMetricsCollector {
  readonly platform: RemoteSystemPlatform
  collect(executor: SystemMetricsExecutor): Promise<SystemMetrics>
}

export interface SystemMetricsCollectionResult {
  platform: RemoteSystemPlatform
  metrics: SystemMetrics
}
