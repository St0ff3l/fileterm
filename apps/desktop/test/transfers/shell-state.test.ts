import assert from 'node:assert/strict'
import test from 'node:test'
import {
  BUSYBOX_SHELL_CWD_SETUP,
  findSetupEchoEnd,
  resolveShellFileAccess,
  SHELL_CWD_SETUP,
  shellCwdSetupForPlatform,
  ShellCwdTracker,
  supportsPosixShellSetup
} from '../../src/main/services/sessions/shell-cwd-integration.ts'

test('shell state tracker parses chunked cwd and user reports', () => {
  const tracker = new ShellCwdTracker()
  assert.deepEqual(tracker.feed('\u001b]7;file://host/root\u0007\u001b]1337;Remote'), [{ cwd: '/root' }])
  assert.deepEqual(tracker.feed('User=root\u0007'), [{ user: 'root' }])
})

test('shell state tracker rejects control characters in reported users', () => {
  const tracker = new ShellCwdTracker()
  assert.deepEqual(tracker.feed('\u001b]1337;RemoteUser=root\nspoofed\u0007'), [])
})

test('shell user changes map to one-way file access targets', () => {
  assert.deepEqual(resolveShellFileAccess('stoffel', 'stoffel'), { mode: 'user' })
  assert.deepEqual(resolveShellFileAccess('stoffel', 'root'), { mode: 'root', sudoUser: 'root' })
  assert.deepEqual(resolveShellFileAccess('stoffel', 'postgres'), { mode: 'root', sudoUser: 'postgres' })
})

test('shell setup is injected only for confirmed POSIX platforms', () => {
  assert.equal(supportsPosixShellSetup('linux'), true)
  assert.equal(supportsPosixShellSetup('busybox'), true)
  assert.equal(supportsPosixShellSetup('windows'), false)
  assert.equal(supportsPosixShellSetup('unknown'), false)
  assert.equal(supportsPosixShellSetup(undefined), false)
  assert.match(SHELL_CWD_SETUP, /\$\{FISH_VERSION-\}/)
  assert.match(SHELL_CWD_SETUP, /\$\{BASH_VERSION-\}/)
  assert.doesNotMatch(SHELL_CWD_SETUP, /"\$FISH_VERSION"/)
  assert.equal(shellCwdSetupForPlatform('linux'), SHELL_CWD_SETUP)
  assert.equal(shellCwdSetupForPlatform('busybox'), BUSYBOX_SHELL_CWD_SETUP)
  assert.equal(shellCwdSetupForPlatform('windows'), undefined)
  assert.ok(BUSYBOX_SHELL_CWD_SETUP.length < 256)
  assert.match(BUSYBOX_SHELL_CWD_SETUP, /PS1=/)
})

test('shell setup detection consumes repeated hook payloads before the replacement prompt', () => {
  const firstPayload = '\u001b]7;file:///home/stoffel\u0007\u001b]1337;RemoteUser=stoffel\u0007'
  const secondPayload = '\u001b]7;file:///home/stoffel\u0007\u001b]1337;RemoteUser=stoffel\u0007'
  const replacementPrompt = 'stoffel@debian:~$ '
  const output = ` ${SHELL_CWD_SETUP}\r\n${firstPayload}${secondPayload}${replacementPrompt}`
  const echoEnd = findSetupEchoEnd(output)

  assert.ok(echoEnd)
  assert.equal(echoEnd.cwd, '/home/stoffel')
  assert.equal(echoEnd.user, 'stoffel')
  assert.equal(output.slice(echoEnd.payloadEnd), replacementPrompt)
})

test('shell setup detection works when the remote shell does not echo the injected command', () => {
  const payload = '\u001b]7;file:///volume1\u0007\u001b]1337;RemoteUser=admin\u0007'
  const echoEnd = findSetupEchoEnd(`${payload}admin@synology:~$ `)

  assert.ok(echoEnd)
  assert.equal(echoEnd.lineStart, 0)
  assert.equal(echoEnd.cwd, '/volume1')
  assert.equal(echoEnd.user, 'admin')
})
