import assert from 'node:assert/strict'
import test from 'node:test'
import { WorkspaceTransfersState } from '../../src/main/services/workspace/workspace-transfers.ts'

test('failed transfers can resume while completed and canceled transfers stay terminal', () => {
  const transfers = new WorkspaceTransfersState([])
  const failedId = transfers.add('upload', 'failed.bin', { resumable: true })
  assert.equal(transfers.update(failedId, { status: 'failed' }), true)
  assert.equal(transfers.update(failedId, { status: 'running' }), true)

  const doneId = transfers.add('download', 'done.bin')
  assert.equal(transfers.update(doneId, { status: 'done', progress: 100 }), true)
  assert.equal(transfers.update(doneId, { status: 'running' }), false)

  const canceledId = transfers.add('upload', 'canceled.bin')
  assert.equal(transfers.update(canceledId, { status: 'canceled' }), true)
  assert.equal(transfers.update(canceledId, { status: 'running' }), false)
})

test('identical transfer updates do not mutate updatedAt', () => {
  const transfers = new WorkspaceTransfersState([])
  const id = transfers.add('upload', 'same.bin', { resumable: true })
  const updatedAt = transfers.get(id)?.updatedAt
  assert.equal(transfers.update(id, { status: 'running', resumable: true }), false)
  assert.equal(transfers.get(id)?.updatedAt, updatedAt)
})
