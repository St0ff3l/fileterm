import { parseSystemMetrics } from './parser.js'
import type { SystemMetricsCollector } from './types.js'
import { runPowerShellMetricsScript } from './windows-collector.js'

export const windowsCollector: SystemMetricsCollector = {
  platform: 'windows',
  async collect(executor) {
    const raw = await runPowerShellMetricsScript(executor)
    return parseSystemMetrics(raw, 'windows')
  }
}
