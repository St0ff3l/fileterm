import assert from 'node:assert/strict'
import test from 'node:test'
import { probeRemoteSystemPlatform } from '../../src/main/services/sessions/system-metrics/platform-probe.ts'

test('falls back from a failed POSIX probe to CRLF Windows platform output', async () => {
  const commands: string[] = []
  const platform = await probeRemoteSystemPlatform({
    async exec(command) {
      commands.push(command)
      if (command.startsWith('sh -lc')) {
        throw new Error('sh is not available')
      }
      if (command.startsWith('powershell ')) {
        return 'Win32NT\r\n'
      }
      throw new Error(`Unexpected command: ${command}`)
    }
  })

  assert.equal(platform, 'windows')
  assert.equal(commands.length, 2)
})

test('detects BusyBox without attempting Windows commands', async () => {
  const commands: string[] = []
  const platform = await probeRemoteSystemPlatform({
    async exec(command) {
      commands.push(command)
      return '__FILETERM_PROBE_START__\r\nLinux\r\nBusyBox v1.36.1\r\n__FILETERM_PROBE_END__\r\n'
    }
  })

  assert.equal(platform, 'busybox')
  assert.equal(commands.length, 1)
})

test('does not classify a normal Linux shell as BusyBox just because the binary may be installed', async () => {
  let probeCommand = ''
  const platform = await probeRemoteSystemPlatform({
    async exec(command) {
      probeCommand = command
      return '__FILETERM_PROBE_START__\nLinux\n/usr/bin/dash\n__FILETERM_PROBE_END__\n'
    }
  })

  assert.equal(platform, 'linux')
  assert.doesNotMatch(probeCommand, /command -v busybox/)
})
