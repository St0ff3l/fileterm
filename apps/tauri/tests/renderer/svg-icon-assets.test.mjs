import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'
import vm from 'node:vm'
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import ts from 'typescript'

const require = createRequire(import.meta.url)
const root = fileURLToPath(new URL('../../src/renderer/', import.meta.url))
const cache = new Map()

function load(file) {
  file = resolve(root, file)
  if (cache.has(file)) return cache.get(file)
  const exports = {}
  cache.set(file, exports)
  vm.runInNewContext(
    ts.transpileModule(readFileSync(file, 'utf8'), {
      compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require(name) {
        if (name.endsWith('.svg?raw')) {
          return { __esModule: true, default: readFileSync(resolve(dirname(file), name.slice(0, -4)), 'utf8') }
        }
        if (name.startsWith('.')) return load(resolve(dirname(file), `${name}.ts`))
        return require(name)
      }
    }
  )
  return exports
}

const { APP_ICON_ASSETS } = load('features/common/app-icon-assets.ts')
const { MATERIAL_ICON_ASSETS } = load('features/common/material-icon-assets.ts')
const hash = (value) => createHash('sha256').update(value).digest('hex')
const readFixture = (name) => JSON.parse(readFileSync(new URL(`../fixtures/${name}.json`, import.meta.url), 'utf8'))

test('all 57 original AppIcon names retain the exact geometry at seven stroke widths', () => {
  const baseline = readFixture('app-icon-geometry')
  assert.equal(Object.keys(baseline).length, 57)
  for (const [name, widths] of Object.entries(baseline)) {
    const asset = APP_ICON_ASSETS[name]
    assert.equal(asset.viewBox, '0 0 16 16')
    for (const [width, expected] of Object.entries(widths)) {
      const content = asset.content
        .replaceAll('var(--app-icon-stroke-width, 1.8)', width)
        .replaceAll('var(--app-icon-scaled-stroke-width, 28.8)', String(Number(width) * 16))
      assert.equal(hash(content), expected, `${name} at stroke width ${width}`)
    }
  }
  assert.deepEqual(APP_ICON_ASSETS.brand, APP_ICON_ASSETS.terminal)
})

test('Material SVGs preserve all 58 used font glyph outlines and the original baseline', () => {
  const { glyphs, unitsPerEm, ascent, descent } = readFixture('material-icon-geometry')
  assert.equal(Object.keys(glyphs).length, 58)
  assert.equal(Object.keys(MATERIAL_ICON_ASSETS).length, 58)
  assert.deepEqual([unitsPerEm, ascent, descent], [960, 1056, -96])
  for (const [name, expected] of Object.entries(glyphs)) {
    const asset = MATERIAL_ICON_ASSETS[name]
    assert.equal(asset.viewBox, '0 -960 960 960')
    assert.equal(hash(asset.content.match(/<path d="([^"]+)"/)[1]), expected, name)
  }
})

test('window controls retain their 10px coordinate system and caller-selected stroke width', () => {
  for (const name of ['window-minimize', 'window-maximize', 'window-restore']) {
    assert.equal(APP_ICON_ASSETS[name].viewBox, '0 0 10 10')
    assert.ok(APP_ICON_ASSETS[name].content.includes('var(--app-icon-stroke-width, 1.5)'))
  }
  assert.match(APP_ICON_ASSETS['window-minimize'].content, /x1="1" y1="5" x2="9" y2="5"/)
  assert.match(APP_ICON_ASSETS['window-maximize'].content, /x="1.5" y="1.5" width="7" height="7"/)
})

test('icon components preserve sizing, CSS classes, currentColor and variable stroke parameters', () => {
  const { AppIcon } = load('features/common/app-icon.tsx')
  const { MaterialIcon } = load('components/common/material-icon/material-icon.tsx')
  const icon = renderToStaticMarkup(
    React.createElement(AppIcon, { name: 'copy', size: 20, strokeWidth: 2.2, className: 'fixture' })
  )
  assert.match(icon, /class="app-icon app-icon-copy fixture"/)
  assert.match(icon, /height="20"/)
  assert.match(icon, /width="20"/)
  assert.match(icon, /stroke-width="35.2"/)
  assert.doesNotMatch(icon, /var\(--app-icon/)
  const material = renderToStaticMarkup(
    React.createElement(MaterialIcon, { name: 'settings', className: 'material-symbols-outlined fixture' })
  )
  assert.match(material, /^<span aria-hidden="true" class="material-symbols-outlined fixture">/)
  assert.match(material, /fill="currentColor"/)
  assert.match(material, /width="1em" height="1em"/)
  assert.doesNotMatch(material, />settings</)
})

test('bundled SVG markup contains no executable content or external resources', () => {
  const files = readdirSync(resolve(root, 'assets/icons'), { recursive: true }).filter((file) => file.endsWith('.svg'))
  for (const file of files) {
    const source = readFileSync(resolve(root, 'assets/icons', file), 'utf8')
    assert.match(source, /^<svg\b/)
    assert.doesNotMatch(source, /<(?:script|foreignObject|image|use)\b|\bon\w+\s*=|\b(?:href|src)\s*=/i, file)
  }
  const css = readFileSync(resolve(root, 'styles/material-symbols.css'), 'utf8')
  assert.doesNotMatch(css, /@font-face|url\(/)
})
