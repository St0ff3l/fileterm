import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const script = new URL('../scripts/create-windows-updater-manifest.mjs', import.meta.url)

test('Windows manifest separates installer and portable signed payloads', async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'fileterm-updater-'))
  try {
    for (const [name, signature] of [
      ['FileTerm-9.0.0-windows-x64-setup.exe', 'installer-signature'],
      ['FileTerm-9.0.0-windows-x64-portable.exe', 'portable-signature']
    ]) {
      await fs.writeFile(path.join(directory, name), 'payload')
      await fs.writeFile(path.join(directory, `${name}.sig`), signature)
    }
    const run = () =>
      spawnSync(process.execPath, [fileURLToPath(script), directory], {
        env: { ...process.env, GITHUB_REPOSITORY: 'St0ff3l/fileterm', GITHUB_REF_NAME: 'v9.0.0' },
        encoding: 'utf8'
      })
    assert.equal(run().status, 0)
    const manifest = JSON.parse(await fs.readFile(path.join(directory, 'latest.json'), 'utf8'))
    assert.equal(manifest.version, '9.0.0')
    assert.equal(manifest.platforms['windows-x86_64'].signature, 'installer-signature')
    assert.equal(manifest.platforms['windows-x86_64-portable'].signature, 'portable-signature')
    assert.match(
      manifest.platforms['windows-x86_64-portable'].url,
      /\/v9\.0\.0\/FileTerm-9\.0\.0-windows-x64-portable\.exe$/
    )
    await fs.unlink(path.join(directory, 'FileTerm-9.0.0-windows-x64-portable.exe.sig'))
    assert.notEqual(run().status, 0, 'missing portable signature must stop publication')
  } finally {
    await fs.rm(directory, { recursive: true, force: true })
  }
})
