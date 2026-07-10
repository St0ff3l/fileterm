import assert from 'node:assert/strict'
import test from 'node:test'
import { resolveShellFileAccess, ShellCwdTracker } from '../../src/main/services/sessions/shell-cwd-integration.ts'

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
