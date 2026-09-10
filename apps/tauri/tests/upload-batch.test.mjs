import assert from 'node:assert/strict'
import test from 'node:test'
import { createUploadBatch } from '../src/renderer/hooks/upload-batch.ts'

test('failed folders do not prevent later paths from being queued, duplicates are submitted once', async () => {
  const attempted = []
  const result = await createUploadBatch(['files', 'app.exe', 'shared', 'last.dll', 'files'], async (path) => {
    attempted.push(path)
    if (path === 'files' || path === 'shared') throw new Error(`cannot scan ${path}`)
    return { path }
  })
  assert.deepEqual(attempted, ['files', 'app.exe', 'shared', 'last.dll'])
  assert.deepEqual(result.latestSnapshot, { path: 'last.dll' })
  assert.deepEqual(
    result.failures.map(({ path }) => path),
    ['files', 'shared']
  )
  assert.equal(result.failures[0].error.message, 'cannot scan files')
})

test('a final failure preserves the last successful snapshot', async () => {
  const result = await createUploadBatch(['good', 'bad'], async (path) => {
    if (path === 'bad') throw new Error('denied')
    return path
  })
  assert.equal(result.latestSnapshot, 'good')
  assert.equal(result.failures.length, 1)
})

test('an entirely failed or empty batch has no snapshot to apply', async () => {
  const result = await createUploadBatch(['files', 'shared'], async () => {
    throw new Error('denied')
  })
  assert.equal(result.latestSnapshot, null)
  assert.equal(result.failures.length, 2)
  assert.deepEqual(await createUploadBatch([], async () => 'unused'), { latestSnapshot: null, failures: [] })
})
