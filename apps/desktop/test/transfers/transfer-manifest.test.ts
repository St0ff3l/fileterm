import assert from 'node:assert/strict'
import test from 'node:test'
import {
  createTransferManifest,
  isTransferManifestComplete,
  isValidTransferManifest,
  transferManifestProgress,
  updateTransferManifestEntry
} from '../../src/main/services/transfers/transfer-manifest.ts'

test('directory manifest tracks completed and partial files by byte weight', () => {
  const initial = createTransferManifest(
    ['/remote/root'],
    [
      {
        relativePath: 'one.bin',
        sourcePath: '/local/one.bin',
        destinationPath: '/remote/root/one.bin',
        partialPath: '/remote/root/one.bin.fileterm-part',
        sourceIdentity: { size: 100, modifiedAt: 1 }
      },
      {
        relativePath: 'empty.txt',
        sourcePath: '/local/empty.txt',
        destinationPath: '/remote/root/empty.txt',
        partialPath: '/remote/root/empty.txt.fileterm-part',
        sourceIdentity: { size: 0, modifiedAt: 2 }
      }
    ]
  )
  const partial = updateTransferManifestEntry(initial, 'one.bin', {
    status: 'running',
    transferredBytes: 50
  })

  assert.deepEqual(transferManifestProgress(partial), {
    percent: 50,
    transferredBytes: 50,
    totalBytes: 100
  })
  assert.equal(isTransferManifestComplete(partial), false)

  const completed = updateTransferManifestEntry(
    updateTransferManifestEntry(partial, 'one.bin', { status: 'done', transferredBytes: 100 }),
    'empty.txt',
    { status: 'done', transferredBytes: 0 }
  )
  assert.equal(isTransferManifestComplete(completed), true)
  assert.equal(isValidTransferManifest(completed), true)
})

test('manifest validation rejects unsafe persisted entries', () => {
  assert.equal(isValidTransferManifest({ version: 1, directories: [], files: [{ sourcePath: 1 }] }), false)
})
