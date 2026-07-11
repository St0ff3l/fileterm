import { parseSystemMetrics } from './parser.js'
import { assertPosixMetricsComplete, buildPosixMetricsCommand } from './posix-script.js'
import type { SystemMetricsCollector } from './types.js'

export const busyboxCollector: SystemMetricsCollector = {
  platform: 'busybox',
  async collect(executor) {
    const raw = await executor.exec(
      'sh',
      { allowNonZeroWithStdout: true, timeoutMs: 10000 },
      `${buildPosixMetricsCommand('busybox')}\n`
    )
    assertPosixMetricsComplete(raw, 'BusyBox')
    return parseSystemMetrics(raw, 'busybox')
  }
}
