import assert from 'node:assert/strict'
import { access, mkdtemp, readFile, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import type { StoredSshKey } from '@fileterm/storage'
import { FileSshKeyRepository } from '../../src/main/services/ssh-keys/file-ssh-key-repository.ts'

const KEY_ID = '11111111-1111-4111-8111-111111111111'

async function withRepository(
  run: (repository: FileSshKeyRepository, directory: string) => Promise<void>
): Promise<void> {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-ssh-keys-'))
  try {
    await run(new FileSshKeyRepository(directory), directory)
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

function storedKey(overrides: Partial<StoredSshKey> = {}): StoredSshKey {
  return {
    id: KEY_ID,
    name: 'id_ed25519',
    algorithm: 'ssh-ed25519',
    fingerprint: 'SHA256:test-fingerprint',
    encrypted: false,
    importedAt: 1_700_000_000_000,
    ...overrides
  }
}

test('repository persists metadata and a managed private-key copy', async () => {
  await withRepository(async (repository, directory) => {
    const privateKey = Buffer.from('private-key-content')
    await repository.create(storedKey(), privateKey)

    assert.deepEqual(await repository.list(), [storedKey()])
    assert.deepEqual(Buffer.from(await repository.readPrivateKey(KEY_ID)), privateKey)

    const metadata = JSON.parse(await readFile(path.join(directory, 'ssh-keys.json'), 'utf8')) as {
      keys: StoredSshKey[]
    }
    assert.deepEqual(metadata.keys, [storedKey()])
    assert.equal(JSON.stringify(metadata).includes('private-key-content'), false)
  })
})

test('repository updates notes and stores passphrases outside metadata', async () => {
  await withRepository(async (repository, directory) => {
    await repository.create(storedKey(), Buffer.from('private-key-content'))
    const updated = await repository.updateNote(KEY_ID, '  production  ')
    await repository.setPassphrase(KEY_ID, 'saved-secret')

    assert.equal(updated.note, 'production')
    assert.equal(await repository.getPassphrase(KEY_ID), 'saved-secret')

    const metadata = await readFile(path.join(directory, 'ssh-keys.json'), 'utf8')
    const secrets = await readFile(path.join(directory, 'ssh-key-secrets.json'), 'utf8')
    assert.equal(metadata.includes('saved-secret'), false)
    assert.equal(secrets.includes('saved-secret'), true)

    await repository.setPassphrase(KEY_ID, undefined)
    assert.equal(await repository.getPassphrase(KEY_ID), undefined)
  })
})

test('repository deletes the managed key, metadata and saved passphrase together', async () => {
  await withRepository(async (repository, directory) => {
    await repository.create(storedKey(), Buffer.from('private-key-content'))
    await repository.setPassphrase(KEY_ID, 'saved-secret')
    await repository.delete(KEY_ID)

    assert.deepEqual(await repository.list(), [])
    assert.equal(await repository.getPassphrase(KEY_ID), undefined)
    await assert.rejects(access(path.join(directory, 'ssh-keys', `${KEY_ID}.key`)))
  })
})

test('repository rejects invalid ids before resolving a managed path', async () => {
  await withRepository(async (repository) => {
    await assert.rejects(repository.create(storedKey({ id: '../escape' }), Buffer.from('secret')), /Invalid SSH key id/)
  })
})
