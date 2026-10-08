import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const root = resolve(app, '../..')
const fonts = join(app, 'src/renderer/assets/fonts')
const dist = join(app, 'dist')
const licenses = join(app, 'public/licenses')
const groups = {
  'cascadia-code': 'OFL.txt',
  outfit: 'OFL.txt',
  'jetbrains-mono': 'OFL.txt',
  geist: 'OFL.txt',
  'hanken-grotesk': 'OFL.txt',
  inter: 'OFL.txt',
  'noto-sans-sc': 'OFL.txt',
  'material-symbols': 'LICENSE.txt'
}

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path) : [path]
  })
}

const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
const isFont = (path) => /\.(?:ttf|otf|woff2?)$/i.test(path)
const outputFonts = files(dist).filter(isFont)
const outputHashes = new Set(outputFonts.map(hash))
const sourceFonts = files(fonts).filter(isFont)
const knownHashes = new Set()
const cssPaths = ['text-fonts.css', 'material-symbols.css'].map((name) => join(app, 'src/renderer/styles', name))
const references = new Set()
for (const path of cssPaths) {
  const css = readFileSync(path, 'utf8')
  assert.ok(!/https?:\/\//i.test(css), 'Bundled fonts must not use remote URLs')
  for (const match of css.matchAll(/url\(['"]?([^'"\s)]+)['"]?\)/g)) {
    references.add(resolve(dirname(path), match[1]))
  }
}

const notices = readFileSync(join(root, 'THIRD_PARTY_NOTICES.md'), 'utf8')
assert.equal(readFileSync(join(licenses, 'THIRD_PARTY_NOTICES.md'), 'utf8'), notices, 'Packaged notices must match')
for (const path of sourceFonts) {
  const group = dirname(path).split(/[\\/]/).at(-1)
  assert.ok(groups[group], `Font group has no declared license: ${group}`)
  const license = join(group, groups[group])
  const licenseText = readFileSync(join(licenses, license), 'utf8')
  assert.ok(licenseText.includes(group === 'material-symbols' ? 'Apache License' : 'SIL OPEN FONT LICENSE'))
  assert.ok(notices.includes(`licenses/${group}/${groups[group]}`), `Missing notice for ${group}`)
  assert.ok(references.has(path), `Font is not referenced by CSS: ${path}`)
  const digest = hash(path)
  assert.ok(outputHashes.has(digest), `Font missing or changed in production output: ${path}`)
  knownHashes.add(digest)
}
assert.equal(references.size, sourceFonts.length, 'CSS must reference exactly the inventoried fonts')

// Monaco emits an additional icon font from node_modules, outside our asset tree.
const codicon = join(root, 'node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.ttf')
const codiconHash = hash(codicon)
assert.ok(outputHashes.has(codiconHash), 'Monaco Codicons font must be packaged unchanged')
knownHashes.add(codiconHash)
assert.ok(readFileSync(join(licenses, 'codicons/LICENSE.txt'), 'utf8').includes('Attribution 4.0 International'))
assert.ok(notices.includes('licenses/codicons/LICENSE.txt'), 'Codicons attribution must be included')
for (const name of ['LICENSE', 'ThirdPartyNotices.txt']) {
  assert.equal(hash(join(licenses, 'monaco-editor', name)), hash(join(root, 'node_modules/monaco-editor', name)))
}
for (const path of outputFonts) assert.ok(knownHashes.has(hash(path)), `Undeclared output font: ${path}`)
for (const path of files(licenses)) {
  assert.equal(
    hash(join(dist, 'licenses', relative(licenses, path))),
    hash(path),
    `License missing or changed: ${path}`
  )
}
console.log(`Verified ${sourceFonts.length} bundled fonts + Monaco Codicons, CSS references, and packaged licenses.`)
