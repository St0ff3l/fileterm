import assert from 'node:assert/strict'
import test from 'node:test'
import {
  buildWindowsMetricsScript,
  runPowerShellMetricsScript
} from '../../src/main/services/sessions/system-metrics/windows-collector.ts'

const WINDOWS_METRICS_OUTPUT = [
  '__PLATFORM__windows',
  '__OS__Microsoft Windows Server 2022',
  '__HOSTNAME__WIN-COLLECTOR',
  '__FILETERM_METRICS_COMPLETE__'
].join('\r\n')

test('runs the bounded Windows fallback collector as a compressed PowerShell command', async () => {
  const raw = await runPowerShellMetricsScript({
    async exec(command, options, stdin) {
      assert.match(command, /^powershell /)
      assert.match(command, /FromBase64String/)
      assert.match(command, /GzipStream/)
      assert.ok(command.length < 8000)
      assert.deepEqual(options, { timeoutMs: 12000 })
      assert.equal(stdin, undefined)
      return WINDOWS_METRICS_OUTPUT
    }
  })

  assert.equal(raw, WINDOWS_METRICS_OUTPUT)
  const script = buildWindowsMetricsScript()
  assert.match(script, /^\s*& \{/)
  assert.match(script, /Get-CimInstance/)
  assert.match(script, /Get-WmiObject/)
  assert.match(script, /CollectionDeadline = \(Get-Date\)\.AddSeconds\(8\)/)
  assert.match(script, /Wait-Job -Job \$job -Timeout 2/)
  assert.match(script, /wmic\.exe/)
  assert.match(script, /WaitForExit\(2000\)/)
  assert.match(script, /ipconfig\.exe/)
  assert.match(script, /tasklist\.exe/)
  assert.match(script, /env:SSH_CONNECTION/)
  assert.match(script, /sshConnectionParts\[2\]/)
  assert.match(script, /tailscale\|zerotier\|vethernet/)
  assert.match(script, /Sort-Object -Property Score -Descending/)
  assert.match(script, /processCpuPercentById/)
  assert.match(script, /cpuDelta \* 100000/)
  assert.match(script, /Write-Metric "LOAD" \$systemLoad/)
  assert.match(script, /Write-Metric "LOAD_UNIT" "busy-logical-processors"/)
  assert.match(script, /memoryUsedBytes = \[math\]::Max\(\[double\] 0/)
  assert.match(script, /__FILETERM_METRICS_COMPLETE__"\s*\n}/)
})

test('falls back to pwsh only when Windows PowerShell is unavailable', async () => {
  const commands: string[] = []
  const raw = await runPowerShellMetricsScript({
    async exec(command) {
      commands.push(command)
      if (command.startsWith('powershell ')) {
        throw new Error("'powershell' is not recognized as an internal or external command")
      }
      return WINDOWS_METRICS_OUTPUT
    }
  })

  assert.equal(raw, WINDOWS_METRICS_OUTPUT)
  assert.equal(commands.length, 2)
  assert.match(commands[1] ?? '', /^pwsh /)
})

test('does not rerun an incomplete PowerShell collector through pwsh', async () => {
  const commands: string[] = []
  await assert.rejects(
    runPowerShellMetricsScript({
      async exec(command) {
        commands.push(command)
        return '__PLATFORM__windows\r\n'
      }
    }),
    /did not emit/
  )

  assert.equal(commands.length, 1)
})

test('does not try pwsh after a PowerShell timeout', async () => {
  const commands: string[] = []
  await assert.rejects(
    runPowerShellMetricsScript({
      async exec(command) {
        commands.push(command)
        const error = new Error('命令执行超时')
        error.name = 'TimeoutError'
        throw error
      }
    }),
    /超时/
  )

  assert.equal(commands.length, 1)
  assert.match(commands[0] ?? '', /^powershell /)
})
