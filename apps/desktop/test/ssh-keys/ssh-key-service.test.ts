import assert from 'node:assert/strict'
import { generateKeyPairSync } from 'node:crypto'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { MemoryProfileRepository } from '@fileterm/storage'
import { utils } from 'ssh2'
import { FileSshKeyRepository } from '../../src/main/services/ssh-keys/file-ssh-key-repository.ts'
import { SshKeyService } from '../../src/main/services/ssh-keys/ssh-key-service.ts'

async function withService(
  run: (context: { service: SshKeyService; profiles: MemoryProfileRepository; directory: string }) => Promise<void>
): Promise<void> {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-ssh-key-service-'))
  const profiles = new MemoryProfileRepository([])
  try {
    await run({
      service: new SshKeyService(new FileSshKeyRepository(directory), profiles),
      profiles,
      directory
    })
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

function generatePrivateKey(passphrase?: string) {
  return generateKeyPairSync('rsa', {
    modulusLength: 2048,
    privateKeyEncoding: passphrase
      ? { type: 'pkcs8', format: 'pem', cipher: 'aes-256-cbc', passphrase }
      : { type: 'pkcs8', format: 'pem' },
    publicKeyEncoding: { type: 'spki', format: 'pem' }
  }).privateKey
}

async function writeKeyFile(directory: string, name: string, content: string | Buffer) {
  const filePath = path.join(directory, name)
  await writeFile(filePath, content)
  return filePath
}

test('service imports a valid private key and deduplicates its fingerprint', async () => {
  await withService(async ({ service, directory }) => {
    const sourcePath = await writeKeyFile(directory, 'id_rsa.pem', generatePrivateKey())
    const first = await service.import({ sourcePath, note: 'production' })
    const duplicate = await service.import({ sourcePath, note: 'ignored duplicate note' })

    assert.ok(first)
    assert.ok(duplicate)
    assert.equal(first.duplicate, false)
    assert.equal(duplicate.duplicate, true)
    assert.equal(duplicate.key.id, first.key.id)
    assert.equal(first.key.note, 'production')
    assert.match(first.key.fingerprint, /^SHA256:/)
    assert.equal((await service.list()).length, 1)
    assert.deepEqual(Object.keys(first.key).sort(), [
      'algorithm',
      'encrypted',
      'fingerprint',
      'id',
      'importedAt',
      'name',
      'note',
      'usageCount'
    ])
  })
})

test('service recognizes encrypted private keys without exposing their content', async () => {
  await withService(async ({ service, directory }) => {
    const sourcePath = await writeKeyFile(directory, 'encrypted.pem', generatePrivateKey('secret-passphrase'))
    const result = await service.import({ sourcePath })

    assert.ok(result)
    assert.equal(result.key.encrypted, true)
    assert.equal(result.key.algorithm, 'encrypted')
    assert.match(result.key.fingerprint, /^FILE-SHA256:/)
    assert.equal(JSON.stringify(result).includes('BEGIN ENCRYPTED PRIVATE KEY'), false)
  })
})

test('service rejects invalid files and SSH public keys', async () => {
  await withService(async ({ service, directory }) => {
    const invalidPath = await writeKeyFile(directory, 'invalid.key', 'not a private key')
    await assert.rejects(service.import({ sourcePath: invalidPath }), /无法识别/)

    const privateKey = generatePrivateKey()
    const parsed = utils.parseKey(privateKey)
    assert.equal(parsed instanceof Error, false)
    if (parsed instanceof Error) return
    const publicPath = await writeKeyFile(
      directory,
      'id_rsa.pub',
      `${parsed.type} ${parsed.getPublicSSH().toString('base64')} generated@test\n`
    )
    await assert.rejects(service.import({ sourcePath: publicPath }), /公钥|无法识别/)
  })
})

test('service reports usage and blocks deleting a referenced key', async () => {
  await withService(async ({ service, profiles, directory }) => {
    const sourcePath = await writeKeyFile(directory, 'id_rsa.pem', generatePrivateKey())
    const imported = await service.import({ sourcePath })
    assert.ok(imported)

    await profiles.create({
      type: 'ssh',
      name: 'Production server',
      host: 'server.example.com',
      port: 22,
      username: 'operator',
      group: '默认',
      remotePath: '/home/operator',
      authType: 'privateKey',
      privateKeyId: imported.key.id
    })

    assert.equal((await service.list())[0].usageCount, 1)
    await assert.rejects(service.delete(imported.key.id), /Production server/)
    assert.equal((await service.list()).length, 1)
  })
})
