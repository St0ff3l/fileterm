import assert from 'node:assert/strict'
import test from 'node:test'
import { decodeServerSavedViews } from '../../src/renderer/features/server-management/server-saved-views-model.ts'

test('saved views round-trip their search, tag and session status, and reject corrupt data', () => {
  const view = { id: 'view-1', name: '生产 SSH', tag: '生产', query: 'web', status: 'connected' }
  assert.deepEqual(decodeServerSavedViews(null), [])
  assert.deepEqual(decodeServerSavedViews(JSON.stringify([view])), [view])
  assert.throws(() => decodeServerSavedViews(JSON.stringify([view, view])))
  assert.throws(() => decodeServerSavedViews(JSON.stringify([{ ...view, status: 'unknown' }])))
})
