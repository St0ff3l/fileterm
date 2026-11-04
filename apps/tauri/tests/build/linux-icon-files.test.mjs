import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const root = fileURLToPath(new URL('../../', import.meta.url))
const script = join(root, 'scripts/verify-linux-icon-files.py')
const python = process.platform === 'win32' ? 'python' : 'python3'

test('Linux package verification checks installed launchers and portable AppImage entries', () => {
  const fixture = mkdtempSync(join(tmpdir(), 'fileterm-package-icons-'))
  const put = (path, contents) => {
    const target = join(fixture, path)
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, contents)
  }
  const run = (...args) => {
    const result = spawnSync(python, [script, fixture, ...args], { encoding: 'utf8' })
    assert.ifError(result.error)
    return result
  }
  try {
    for (const [destination, source] of [
      ['usr/share/pixmaps/fileterm.png', 'app/shared/classic-1024.png'],
      ['usr/share/pixmaps/com.fileterm.desktop.png', 'app/shared/classic-1024.png'],
      ['usr/share/icons/hicolor/32x32/apps/fileterm.png', 'app/linux/32x32.png'],
      ['usr/share/icons/hicolor/128x128/apps/fileterm.png', 'app/linux/128x128.png'],
      ['usr/share/icons/hicolor/256x256@2/apps/fileterm.png', 'app/linux/128x128@2x.png'],
      ['usr/share/icons/hicolor/1024x1024/apps/fileterm.png', 'app/shared/classic-1024.png']
    ]) {
      put(destination, readFileSync(join(root, 'src-tauri/icons', source)))
    }
    for (const name of ['FileTerm.desktop', 'com.fileterm.desktop.desktop']) {
      put(`usr/share/applications/${name}`, readFileSync(join(root, 'src-tauri/linux', name)))
    }
    assert.equal(run().status, 0)
    const icon = 'usr/share/pixmaps/fileterm.png'
    const expected = readFileSync(join(fixture, icon))
    put(icon, Buffer.from('stale icon'))
    assert.match(run().stderr, /Stale or modified package icon/)
    put(icon, expected)
    const companion = 'usr/share/applications/com.fileterm.desktop.desktop'
    const desktop = readFileSync(join(fixture, companion), 'utf8')
    put(companion, desktop.replace('Exec=/usr/bin/fileterm %U', 'Exec=/usr/lib/fileterm/runtime/fileterm %U'))
    assert.match(run().stderr, /Incorrect Exec/)
    put(companion, desktop.replace('NoDisplay=true', 'NoDisplay=false'))
    assert.match(run().stderr, /Incorrect launcher visibility/)
    put(companion, desktop.replaceAll('\n', '\r\n'))
    assert.match(run().stderr, /must use LF line endings/)
    rmSync(join(fixture, companion))
    assert.notEqual(run().status, 0)

    put(companion, desktop)
    assert.match(run('--appimage').stderr, /Incorrect Exec/)
    const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.release.linux.conf.json'), 'utf8'))
    for (const name of ['FileTerm.desktop', 'com.fileterm.desktop.desktop']) {
      const source = config.bundle.linux.appimage.files[`/usr/share/applications/${name}`]
      put(`usr/share/applications/${name}`, readFileSync(join(root, 'src-tauri', source)))
    }
    const appimageDesktop = readFileSync(join(fixture, 'usr/share/applications/FileTerm.desktop'))
    put('FileTerm.desktop', appimageDesktop)
    const rootIcon = readFileSync(join(root, 'src-tauri/icons/app/shared/classic-1024.png'))
    put('fileterm.png', rootIcon)
    put('.DirIcon', rootIcon)
    for (const executable of ['AppRun', 'AppRun.wrapped', 'usr/bin/fileterm']) put(executable, 'fixture executable')
    assert.equal(run('--appimage').status, 0)

    put('com.fileterm.desktop.desktop', desktop)
    assert.match(run('--appimage').stderr, /exactly one root desktop entry/)
    rmSync(join(fixture, 'com.fileterm.desktop.desktop'))
    put('FileTerm.desktop', Buffer.concat([appimageDesktop, Buffer.from('# stale root entry\n')]))
    assert.match(run('--appimage').stderr, /root desktop entry differs/)
    put('FileTerm.desktop', appimageDesktop)
    const lowResolutionIcon = readFileSync(join(root, 'src-tauri/icons/app/linux/128x128.png'))
    put('fileterm.png', lowResolutionIcon)
    const appimageWithCompatibilityIcon = run('--appimage')
    assert.equal(appimageWithCompatibilityIcon.status, 0, appimageWithCompatibilityIcon.stderr)
    put('.DirIcon', lowResolutionIcon)
    assert.match(run('--appimage').stderr, /AppDir icon must be 1024x1024px: \.DirIcon is 128x128/)
    put('.DirIcon', rootIcon)
    rmSync(join(fixture, 'usr/bin/fileterm'))
    assert.match(run('--appimage').stderr, /AppDir executable is missing/)
  } finally {
    rmSync(fixture, { recursive: true, force: true })
  }
})
