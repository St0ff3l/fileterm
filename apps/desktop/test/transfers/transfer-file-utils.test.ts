import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import {
  localTransferPartialPath,
  replaceLocalFile,
  sameTransferIdentity,
  statLocalFile
} from '../../src/main/services/transfers/transfer-file-utils.ts'

test('sameTransferIdentity rejects changed files', () => {
  assert.equal(sameTransferIdentity({ size: 10, modifiedAt: 100 }, { size: 10, modifiedAt: 100 }), true)
  assert.equal(sameTransferIdentity({ size: 11, modifiedAt: 100 }, { size: 10, modifiedAt: 100 }), false)
  assert.equal(sameTransferIdentity({ size: 10, modifiedAt: 101 }, { size: 10, modifiedAt: 100 }), false)
  assert.equal(sameTransferIdentity({ size: 10 }, { size: 10 }), true)
})

test('replaceLocalFile replaces an existing destination and removes the checkpoint', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-transfer-'))
  try {
    const destination = path.join(directory, 'payload.bin')
    const partial = localTransferPartialPath(destination)
    await writeFile(destination, 'old')
    await writeFile(partial, 'new payload')
    await replaceLocalFile(partial, destination)
    assert.equal(await readFile(destination, 'utf8'), 'new payload')
    assert.equal(await statLocalFile(partial), null)
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})
test('replaceLocalFile rolls the destination back when final rename fails', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-transfer-'))
  try {
    const destination = path.join(directory, 'payload.bin')
    const missingPartial = localTransferPartialPath(destination)
    await writeFile(destination, 'keep me')
    await assert.rejects(() => replaceLocalFile(missingPartial, destination))
    assert.equal(await readFile(destination, 'utf8'), 'keep me')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})
