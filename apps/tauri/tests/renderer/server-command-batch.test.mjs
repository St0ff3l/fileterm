import assert from 'node:assert/strict'
import test from 'node:test'
import { getCommandBatchTargets } from '../../src/renderer/features/server-management/server-command-batch-model.ts'

test('batch selection binds one eligible SSH session per profile to its current revision', () => {
  const tab = (id, profileId, extra) => ({ id, profileId, sessionType: 'ssh', ...extra })
  const session = (profileId, extra) => ({
    profileId,
    connected: true,
    capabilities: { shellIntegration: true },
    aiSessionRevision: '11',
    ...extra
  })
  const tabs = [
    tab('offline', 'a'),
    tab('visible', 'a'),
    tab('duplicate', 'a'),
    tab('overview-background', 'b', { isBackground: true }),
    tab('external-background', 'c', { isBackground: true, source: 'mcp' }),
    tab('device', 'd'),
    tab('exec-disabled', 'e'),
    tab('stale', 'f'),
    tab('wrong-type', 'g', { sessionType: 'telnet' })
  ]
  const sessions = {
    offline: session('a', { connected: false }),
    visible: session('a'),
    duplicate: session('a', { aiSessionRevision: '12' }),
    'overview-background': session('b', { aiSessionRevision: '13' }),
    'external-background': session('c'),
    device: session('d', { deviceMode: 'network-device' }),
    'exec-disabled': session('e', { capabilities: { shellIntegration: false } }),
    stale: session('different-profile'),
    'wrong-type': session('g')
  }

  assert.deepEqual(
    [...getCommandBatchTargets(tabs, sessions).values()],
    [
      { profileId: 'a', tabId: 'visible', sessionRevision: '11' },
      { profileId: 'b', tabId: 'overview-background', sessionRevision: '13' }
    ]
  )
})
