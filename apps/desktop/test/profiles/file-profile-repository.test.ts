import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import type { ConnectionProfile, CreateProfileInput } from '@fileterm/core'
import { FileProfileRepository } from '../../src/main/services/file-profile-repository.ts'

async function withRepository(
  run: (repository: FileProfileRepository, directory: string) => Promise<void>
): Promise<void> {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-profiles-'))
  try {
    await run(new FileProfileRepository(directory, []), directory)
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

function createSshProfileInput(group: string, name: string): CreateProfileInput {
  return {
    type: 'ssh',
    name,
    host: 'server.example.com',
    port: 22,
    username: 'operator',
    authType: 'system',
    group,
    remotePath: '/home/operator'
  }
}

async function readStoredProfiles(directory: string): Promise<ConnectionProfile[]> {
  const content = await readFile(path.join(directory, 'profiles.json'), 'utf8')
  return JSON.parse(content) as ConnectionProfile[]
}

async function writeStoredProfiles(directory: string, profiles: ConnectionProfile[]): Promise<void> {
  await writeFile(path.join(directory, 'profiles.json'), JSON.stringify(profiles, null, 2), 'utf8')
}

function findProfile(profiles: ConnectionProfile[], id: string): ConnectionProfile {
  const profile = profiles.find((item) => item.id === id)
  assert.ok(profile, `Expected profile ${id} to exist`)
  return profile
}

test('privateKeyId persists independently from the legacy privateKeyPath', async () => {
  await withRepository(async (repository, directory) => {
    const profile = await repository.create({
      ...createSshProfileInput('默认', 'Managed key server'),
      authType: 'privateKey',
      privateKeyId: '11111111-1111-4111-8111-111111111111'
    })

    assert.equal(profile.type, 'ssh')
    if (profile.type !== 'ssh') return
    assert.equal(profile.privateKeyId, '11111111-1111-4111-8111-111111111111')
    assert.equal(profile.privateKeyPath, undefined)

    const storedProfile = findProfile(await readStoredProfiles(directory), profile.id)
    assert.equal(storedProfile.type, 'ssh')
    if (storedProfile.type !== 'ssh') return
    assert.equal(storedProfile.privateKeyId, '11111111-1111-4111-8111-111111111111')
    assert.equal(storedProfile.privateKeyPath, undefined)
  })
})

test('create resolves parentId from the selected group name', async () => {
  await withRepository(async (repository, directory) => {
    const folder = await repository.createFolder('Production')
    const profile = await repository.create(createSshProfileInput('Production', 'Primary server'))

    assert.equal(profile.group, 'Production')
    assert.equal(profile.parentId, folder.id)

    const storedProfile = findProfile(await readStoredProfiles(directory), profile.id)
    assert.equal(storedProfile.group, 'Production')
    assert.equal(storedProfile.parentId, folder.id)
  })
})

test('renaming a folder cascades its name to child profile groups', async () => {
  await withRepository(async (repository, directory) => {
    const folder = await repository.createFolder('Production')
    const profile = await repository.create(createSshProfileInput('Production', 'Primary server'))

    await repository.updateFolder(folder.id, { name: 'Critical' })

    const updatedProfile = findProfile(await repository.list(), profile.id)
    assert.equal(updatedProfile.group, 'Critical')
    assert.equal(updatedProfile.parentId, folder.id)

    const storedProfile = findProfile(await readStoredProfiles(directory), profile.id)
    assert.equal(storedProfile.group, 'Critical')
    assert.equal(storedProfile.parentId, folder.id)
  })
})

test('deleting nested and top-level folders falls children back to the correct parent', async () => {
  await withRepository(async (repository) => {
    const rootFolder = await repository.createFolder('Root')
    const nestedFolder = await repository.createFolder('Nested', rootFolder.id)
    const descendantFolder = await repository.createFolder('Descendant', nestedFolder.id)
    const rootProfile = await repository.create(createSshProfileInput('Root', 'Root server'))
    const nestedProfile = await repository.create(createSshProfileInput('Nested', 'Nested server'))

    await repository.deleteFolder(nestedFolder.id)

    const profilesAfterNestedDelete = await repository.list()
    const nestedProfileAfterDelete = findProfile(profilesAfterNestedDelete, nestedProfile.id)
    assert.equal(nestedProfileAfterDelete.parentId, rootFolder.id)
    assert.equal(nestedProfileAfterDelete.group, 'Root')

    const foldersAfterNestedDelete = await repository.listFolders()
    assert.equal(
      foldersAfterNestedDelete.some((folder) => folder.id === nestedFolder.id),
      false
    )
    assert.equal(foldersAfterNestedDelete.find((folder) => folder.id === descendantFolder.id)?.parentId, rootFolder.id)

    await repository.deleteFolder(rootFolder.id)

    const profilesAfterRootDelete = await repository.list()
    for (const profileId of [rootProfile.id, nestedProfile.id]) {
      const profile = findProfile(profilesAfterRootDelete, profileId)
      assert.equal(profile.parentId, undefined)
      assert.equal(profile.group, '默认')
    }

    const foldersAfterRootDelete = await repository.listFolders()
    assert.equal(
      foldersAfterRootDelete.some((folder) => folder.id === rootFolder.id),
      false
    )
    assert.equal(foldersAfterRootDelete.find((folder) => folder.id === descendantFolder.id)?.parentId, undefined)
  })
})

test('list heals inconsistent group and parentId values and persists the healed profiles before returning', async () => {
  await withRepository(async (repository, directory) => {
    const alphaFolder = await repository.createFolder('Alpha')
    const betaFolder = await repository.createFolder('Beta')
    const groupWins = await repository.create(createSshProfileInput('Alpha', 'Group authoritative'))
    const parentWins = await repository.create(createSshProfileInput('默认', 'Parent authoritative'))
    const orphaned = await repository.create(createSshProfileInput('默认', 'Orphaned'))

    const inconsistentProfiles = (await readStoredProfiles(directory)).map((profile) => {
      if (profile.id === groupWins.id) {
        return { ...profile, group: 'Alpha', parentId: betaFolder.id }
      }
      if (profile.id === parentWins.id) {
        return { ...profile, group: '默认', parentId: betaFolder.id }
      }
      if (profile.id === orphaned.id) {
        return { ...profile, group: 'Missing folder', parentId: 'missing-folder-id' }
      }
      return profile
    })
    await writeStoredProfiles(directory, inconsistentProfiles)

    const healedProfiles = await repository.list()
    assert.deepEqual(
      [groupWins.id, parentWins.id, orphaned.id].map((id) => {
        const profile = findProfile(healedProfiles, id)
        return { id, group: profile.group, parentId: profile.parentId }
      }),
      [
        { id: groupWins.id, group: 'Alpha', parentId: alphaFolder.id },
        { id: parentWins.id, group: 'Beta', parentId: betaFolder.id },
        { id: orphaned.id, group: '默认', parentId: undefined }
      ]
    )

    const persistedProfiles = await readStoredProfiles(directory)
    assert.deepEqual(
      [groupWins.id, parentWins.id, orphaned.id].map((id) => {
        const profile = findProfile(persistedProfiles, id)
        return { id, group: profile.group, parentId: profile.parentId }
      }),
      [
        { id: groupWins.id, group: 'Alpha', parentId: alphaFolder.id },
        { id: parentWins.id, group: 'Beta', parentId: betaFolder.id },
        { id: orphaned.id, group: '默认', parentId: undefined }
      ]
    )
  })
})

test('keeps proxy credentials out of profiles.json while returning them to the main process', async () => {
  await withRepository(async (repository, directory) => {
    const profile = await repository.create({
      ...createSshProfileInput('默认', 'Proxied server'),
      proxy: { type: 'socks5', host: 'proxy.internal', port: 1080, username: 'proxy-user', password: 'proxy-secret' }
    })

    const publicProfiles = await readStoredProfiles(directory)
    const persisted = findProfile(publicProfiles, profile.id)
    assert.equal('proxy' in persisted && persisted.proxy?.password, undefined)
    assert.equal(
      ((await repository.getById(profile.id)) as ConnectionProfile & { proxy?: { password?: string } }).proxy?.password,
      'proxy-secret'
    )
  })
})

test('persists SSH reconnect mode across an update and reload', async () => {
  await withRepository(async (repository, directory) => {
    const created = await repository.create(createSshProfileInput('默认', 'Reconnect server'))
    const input = { ...createSshProfileInput('默认', 'Reconnect server'), reconnectMode: 'enter' as const }

    const updated = await repository.update(created.id, input)
    assert.equal(updated.type, 'ssh')
    assert.equal(updated.reconnectMode, 'enter')
    assert.equal(findProfile(await readStoredProfiles(directory), created.id).type, 'ssh')
    assert.equal(
      (findProfile(await readStoredProfiles(directory), created.id) as ConnectionProfile).reconnectMode,
      'enter'
    )

    const reloaded = new FileProfileRepository(directory, [])
    const profile = findProfile(await reloaded.list(), created.id)
    assert.equal(profile.type, 'ssh')
    assert.equal(profile.reconnectMode, 'enter')
  })
})
