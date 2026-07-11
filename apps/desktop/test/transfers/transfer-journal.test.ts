import assert from 'node:assert/strict'
import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import type { TransferTask } from '@fileterm/core'
import { TransferJournal } from '../../src/main/services/transfers/transfer-journal.ts'

test('TransferJournal restores active resumable tasks as manually paused', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-journal-'))
  try {
    const journal = new TransferJournal(directory)
    const resumable: TransferTask = {
      id: 'resume-me',
      direction: 'download',
      name: 'archive.tar',
      progress: 42,
      status: 'running',
      tabId: 'tab-1',
      profileId: 'profile-1',
      targetType: 'file',
      sourcePath: '/remote/archive.tar',
      destinationPath: '/local/archive.tar',
      partialPath: '/local/archive.tar.fileterm-part',
      stagingPath: '/tmp/fileterm-root-upload-checkpoint.part',
      resumable: true
    }
    const folder: TransferTask = {
      id: 'folder',
      direction: 'download',
      name: 'folder',
      progress: 12,
      status: 'running',
      profileId: 'profile-1',
      targetType: 'folder',
      sourcePath: '/remote/folder',
      destinationPath: '/local/folder',
      manifest: {
        version: 1,
        directories: ['/local/folder'],
        files: [
          {
            relativePath: 'one.txt',
            sourcePath: '/remote/folder/one.txt',
            destinationPath: '/local/folder/one.txt',
            partialPath: '/local/folder/one.txt.fileterm-part',
            stagingPath: '/tmp/fileterm-root-upload-folder-checkpoint.part',
            sourceIdentity: { size: 10 },
            status: 'running',
            transferredBytes: 4
          }
        ]
      },
      resumable: true
    }
    const legacyInterrupted: TransferTask = {
      ...resumable,
      id: 'legacy-interrupted',
      status: 'interrupted'
    }
    await journal.save([resumable, folder, legacyInterrupted])
    const restored = await journal.load()
    assert.equal(restored[0]?.status, 'paused')
    assert.equal(restored[0]?.tabId, 'tab-1')
    assert.equal(restored[0]?.resumable, true)
    assert.equal(restored[0]?.stagingPath, '/tmp/fileterm-root-upload-checkpoint.part')
    assert.equal(restored[1]?.status, 'paused')
    assert.equal(restored[1]?.resumable, true)
    assert.equal(restored[1]?.manifest?.files[0]?.status, 'pending')
    assert.equal(restored[1]?.manifest?.files[0]?.stagingPath, '/tmp/fileterm-root-upload-folder-checkpoint.part')
    assert.equal(restored[2]?.status, 'paused')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})
