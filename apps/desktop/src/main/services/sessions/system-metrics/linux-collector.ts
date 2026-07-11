import { parseSystemMetrics } from './parser.js'
import { assertPosixMetricsComplete, buildPosixMetricsCommand } from './posix-script.js'
import type { SystemMetricsCollector } from './types.js'

export const linuxCollector: SystemMetricsCollector = {
  platform: 'linux',
  async collect(executor) {
    const raw = await executor.exec(
      'sh',
      { allowNonZeroWithStdout: true, timeoutMs: 10000 },
      `${buildPosixMetricsCommand('linux')}\n`
    )
    assertPosixMetricsComplete(raw, 'Linux')
    return parseSystemMetrics(raw, 'linux')
  }
}
