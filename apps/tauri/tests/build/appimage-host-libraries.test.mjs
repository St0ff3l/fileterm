import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const script = fileURLToPath(new URL('../../../../scripts/appimage-host-libraries.py', import.meta.url))
const python = process.platform === 'win32' ? 'python' : 'python3'

test('AppImage preparation removes conflicting display libraries but preserves bundled app runtimes', () => {
  const fixture = mkdtempSync(join(tmpdir(), 'fileterm appimage graphics '))
  const put = (name, contents = 'fixture') => {
    const path = join(fixture, name)
    mkdirSync(dirname(path), { recursive: true })
    writeFileSync(path, contents)
    return path
  }
  const run = (...args) => {
    const result = spawnSync(python, [script, fixture, ...args], { encoding: 'utf8' })
    assert.ifError(result.error)
    return result
  }
  try {
    put('AppRun')
    const wayland = put('usr/lib/libwayland-client.so.0')
    const versioned = put('usr/lib/x86_64-linux-gnu/libxkbcommon.so.0.0.0')
    const webkit = put('usr/lib/libwebkit2gtk-4.1.so.0', 'WebKit runtime')
    const nearName = put('usr/lib/libwayland-client-extra.so.0')
    assert.notEqual(run().status, 0)
    assert.match(run().stderr, /libwayland-client\.so\.0/)
    if (process.platform !== 'win32') {
      symlinkSync('../libwebkit2gtk-4.1.so.0', join(fixture, 'usr/lib/x86_64-linux-gnu/libxcb-render.so.0'))
    }
    const removed = run('--remove')
    assert.equal(removed.status, 0, removed.stderr)
    assert.equal(existsSync(wayland), false)
    assert.equal(existsSync(versioned), false)
    assert.equal(readFileSync(webkit, 'utf8'), 'WebKit runtime')
    assert.equal(existsSync(nearName), true)
    assert.equal(run().status, 0)
    assert.equal(run('--remove').status, 0)
    rmSync(join(fixture, 'AppRun'))
    assert.notEqual(run('--remove').status, 0)
  } finally {
    rmSync(fixture, { recursive: true, force: true })
  }
})
