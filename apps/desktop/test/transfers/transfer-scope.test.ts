import assert from 'node:assert/strict'
import test from 'node:test'
import type { TransferTask } from '@fileterm/core'
import { scopeTransfersToSession } from '../../src/renderer/features/transfers/transfer-scope.ts'

const transfer = (id: string, profileId: string, tabId?: string): TransferTask => ({
  id,
  direction: 'upload',
  name: `${id}.txt`,
  progress: 0,
  status: 'paused',
  profileId,
  tabId
})

test('transfer scope isolates active sibling tabs and retains reopenable history', () => {
  const transfers = [
    transfer('active', 'profile-1', 'tab-a'),
    transfer('sibling', 'profile-1', 'tab-b'),
    transfer('closed-tab', 'profile-1', 'tab-old'),
    transfer('legacy', 'profile-1'),
    transfer('other-profile', 'profile-2', 'tab-c')
  ]

  const scoped = scopeTransfersToSession(transfers, 'tab-a', 'profile-1', [
    { id: 'tab-a', profileId: 'profile-1' },
    { id: 'tab-b', profileId: 'profile-1' },
    { id: 'tab-c', profileId: 'profile-2' }
  ])

  assert.deepEqual(
    scoped.map((item) => item.id),
    ['active', 'closed-tab', 'legacy']
  )
})

test('transfer scope is empty outside a session tab', () => {
  assert.deepEqual(
    scopeTransfersToSession([transfer('paused', 'profile-1', 'tab-a')], null, 'profile-1', [
      { id: 'tab-a', profileId: 'profile-1' }
    ]),
    []
  )
})
