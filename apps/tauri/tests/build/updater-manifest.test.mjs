import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const script = new URL('../../scripts/create-updater-manifest.mjs', import.meta.url)

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

test('combined manifest includes all twelve Windows, macOS and Linux targets', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'fileterm-updater-merge-'))
  try {
    const architectures = ['x64', 'arm64']
    const directories = []
    for (const architecture of architectures) {
      const directory = path.join(root, architecture)
      directories.push(directory)
      await fs.mkdir(directory)
      for (const [kind, signature] of [
        ['setup', `${architecture}-installer-signature`],
        ['portable', `${architecture}-portable-signature`]
      ]) {
        const filename = `FileTerm-9.0.0-windows-${architecture}-${kind}.exe`
        await fs.writeFile(path.join(directory, filename), 'payload')
        await fs.writeFile(path.join(directory, `${filename}.sig`), signature)
      }
    }

    for (const architecture of architectures) {
      const directory = path.join(root, `macos-${architecture}`)
      directories.push(directory)
      await fs.mkdir(directory)
      const archive = `FileTerm-9.0.0-macos-${architecture}.app.tar.gz`
      await fs.writeFile(path.join(directory, archive), 'payload')
      await fs.writeFile(path.join(directory, `${archive}.sig`), `${architecture}-macos-signature`)
    }

    for (const architecture of ['x86_64', 'arm64']) {
      const directory = path.join(root, `linux-${architecture}`)
      directories.push(directory)
      for (const format of ['AppImage', 'deb', 'rpm']) {
        const nested = path.join(directory, format.toLowerCase())
        await fs.mkdir(nested, { recursive: true })
        const asset = `FileTerm-9.0.0-linux-${architecture}.${format}`
        await fs.writeFile(path.join(nested, asset), 'payload')
        await fs.writeFile(path.join(nested, `${asset}.sig`), `${architecture}-${format}-signature`)
      }
    }

    const result = spawnSync(process.execPath, [fileURLToPath(script), '--merge', ...directories], {
      cwd: root,
      env: { ...process.env, GITHUB_REPOSITORY: 'St0ff3l/fileterm', GITHUB_REF_NAME: 'v9.0.0' },
      encoding: 'utf8'
    })
    assert.equal(result.status, 0, result.stderr)

    const manifest = JSON.parse(await fs.readFile(path.join(root, 'latest.json'), 'utf8'))
    assert.equal(Object.keys(manifest.platforms).length, 12)
    assert.equal(manifest.platforms['darwin-aarch64'].signature, 'arm64-macos-signature')
    assert.equal(manifest.platforms['darwin-x86_64'].signature, 'x64-macos-signature')
    assert.equal(manifest.platforms['windows-x86_64'].signature, 'x64-installer-signature')
    assert.equal(manifest.platforms['windows-x86_64-portable'].signature, 'x64-portable-signature')
    assert.equal(manifest.platforms['windows-aarch64'].signature, 'arm64-installer-signature')
    assert.equal(manifest.platforms['windows-aarch64-portable'].signature, 'arm64-portable-signature')
    assert.match(manifest.platforms['windows-aarch64'].url, /windows-arm64-setup\.exe$/)
    assert.match(manifest.platforms['windows-aarch64-portable'].url, /windows-arm64-portable\.exe$/)
    for (const [architecture, targetArch] of [
      ['x86_64', 'x86_64'],
      ['arm64', 'aarch64']
    ]) {
      assert.equal(manifest.platforms[`linux-${targetArch}`], undefined, 'Linux must never fall back across formats')
      for (const format of ['AppImage', 'deb', 'rpm']) {
        const target = manifest.platforms[`linux-${targetArch}-${format.toLowerCase()}`]
        assert.equal(target.signature, `${architecture}-${format}-signature`)
        assert.equal(
          new URL(target.url).pathname,
          `/St0ff3l/fileterm/releases/download/v9.0.0/FileTerm-9.0.0-linux-${architecture}.${format}`
        )
      }
    }
  } finally {
    await fs.rm(root, { recursive: true, force: true })
  }
})

test('Linux manifest refuses unsigned, incomplete, duplicate or incorrectly named payloads', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'fileterm-linux-updater-'))
  try {
    const formats = ['AppImage', 'deb', 'rpm']
    for (const format of formats) {
      const asset = path.join(root, `FileTerm-9.0.0-linux-x86_64.${format}`)
      await fs.writeFile(asset, 'payload')
      await fs.writeFile(`${asset}.sig`, 'signature')
    }
    const run = (directories = [root]) =>
      spawnSync(process.execPath, [fileURLToPath(script), '--merge', ...directories], {
        cwd: root,
        env: { ...process.env, GITHUB_REPOSITORY: 'St0ff3l/fileterm', GITHUB_REF_NAME: 'v9.0.0' },
        encoding: 'utf8'
      })
    assert.equal(run().status, 0)
    assert.notEqual(run([root, root]).status, 0, 'duplicate target must stop publication')
    for (const format of formats) {
      const asset = path.join(root, `FileTerm-9.0.0-linux-x86_64.${format}`)
      await fs.writeFile(asset, '')
      assert.notEqual(run().status, 0, `empty ${format} payload must stop publication`)
      await fs.unlink(asset)
      await fs.mkdir(asset)
      assert.notEqual(run().status, 0, `a directory cannot be published as ${format}`)
      await fs.rmdir(asset)
      await fs.writeFile(asset, 'payload')
      const signature = path.join(root, `FileTerm-9.0.0-linux-x86_64.${format}.sig`)
      await fs.writeFile(signature, '  ')
      assert.notEqual(run().status, 0, `empty ${format} signature must stop publication`)
      await fs.unlink(signature)
      assert.notEqual(run().status, 0, `missing ${format} signature must stop publication`)
      await fs.writeFile(signature, 'signature')
    }
    const asset = path.join(root, 'FileTerm-9.0.0-linux-x86_64.deb')
    for (const invalid of [
      'FileTerm-8.0.0-linux-x86_64.deb',
      'FileTerm-9.0.0-linux-riscv64.deb',
      'FileTerm-9.0.0-linux-arm64.deb'
    ]) {
      await fs.rename(asset, path.join(root, invalid))
      await fs.copyFile(`${asset}.sig`, path.join(root, `${invalid}.sig`))
      assert.notEqual(run().status, 0, `invalid/mixed package must stop publication: ${invalid}`)
      await fs.unlink(path.join(root, `${invalid}.sig`))
      await fs.rename(path.join(root, invalid), asset)
    }
    await fs.unlink(asset)
    assert.notEqual(run().status, 0, 'missing format must stop publication')
  } finally {
    await fs.rm(root, { recursive: true, force: true })
  }
})

test('combined manifest adds both signed macOS architectures and rejects missing or empty signatures', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'fileterm-macos-updater-'))
  try {
    const directories = []
    for (const architecture of ['arm64', 'x64']) {
      const directory = path.join(root, architecture)
      directories.push(directory)
      await fs.mkdir(directory)
      const archive = `FileTerm-9.0.0-macos-${architecture}.app.tar.gz`
      await fs.writeFile(path.join(directory, archive), 'payload')
      await fs.writeFile(path.join(directory, `${archive}.sig`), `${architecture}-signature`)
    }
    const run = (inputs = directories) =>
      spawnSync(process.execPath, [fileURLToPath(script), '--merge', ...inputs], {
        cwd: root,
        env: { ...process.env, GITHUB_REPOSITORY: 'St0ff3l/fileterm', GITHUB_REF_NAME: 'v9.0.0' },
        encoding: 'utf8'
      })
    const result = run()
    assert.equal(result.status, 0, result.stderr)
    const manifest = JSON.parse(await fs.readFile(path.join(root, 'latest.json'), 'utf8'))
    assert.equal(manifest.platforms['darwin-aarch64'].signature, 'arm64-signature')
    assert.equal(manifest.platforms['darwin-x86_64'].signature, 'x64-signature')
    assert.match(manifest.platforms['darwin-aarch64'].url, /\/v9\.0\.0\/FileTerm-9\.0\.0-macos-arm64\.app\.tar\.gz$/)
    assert.notEqual(run([directories[0], directories[0]]).status, 0, 'duplicate architecture must stop publication')
    const signature = path.join(directories[1], 'FileTerm-9.0.0-macos-x64.app.tar.gz.sig')
    await fs.writeFile(signature, '  ')
    assert.notEqual(run().status, 0, 'empty signature must stop publication')
    await fs.unlink(signature)
    assert.notEqual(run().status, 0, 'missing signature must stop publication')
  } finally {
    await fs.rm(root, { recursive: true, force: true })
  }
})
