import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-webgl-textures.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)
const { registerWebglTextureSlots } = exports

test('merged and replaced atlases release unused slots and all old mip levels without per-frame churn', () => {
  let next = 0
  const deleted = []
  const images = []
  const gl = {
    createTexture: () => ({ id: ++next }),
    activeTexture() {},
    bindTexture() {},
    texParameteri() {},
    texImage2D: (...args) => images.push(args),
    deleteTexture: (texture) => deleted.push(texture)
  }
  const slots = Array.from({ length: 4 }, () => ({ texture: gl.createTexture(), version: 1 }))
  const page = (width, height = width) => ({ canvas: { width, height } })
  const calls = []
  const glyph = {
    _atlasTextures: slots,
    _atlas: { pages: [page(1024), page(1024), page(1024), page(1024)] },
    render(model) {
      assert.equal(this, glyph)
      calls.push(model)
    }
  }
  const original = glyph.render
  const adapter = registerWebglTextureSlots(gl, glyph)
  glyph.render('initial')
  assert.equal(deleted.length, 4)
  const oldSlots = slots.map((slot) => slot.texture)
  glyph.render('unchanged')
  assert.equal(deleted.length, 4)
  glyph._atlas.pages = [page(2048)]
  glyph.render('merged')
  assert.equal(deleted.length, 8)
  assert.deepEqual(deleted.slice(4), oldSlots)
  assert.equal(images.length, 3, 'unused samplers must remain complete 1x1 placeholders')
  assert.ok(slots.every((slot) => slot.version === -1))
  glyph._atlas = { pages: [page(512, 256)] }
  glyph.render('font change')
  assert.equal(deleted.length, 9, 'shrinking either dimension replaces the whole texture including old mips')
  glyph.render('stable')
  assert.equal(deleted.length, 9)
  glyph._atlas.pages.push(page(512))
  glyph.render('reuse')
  assert.equal(deleted.length, 10)
  adapter.dispose()
  assert.equal(glyph.render, original)
  assert.deepEqual(calls, ['initial', 'unchanged', 'merged', 'font change', 'stable', 'reuse'])
  // Upstream disposal closes over each mutable slot, so it must delete the replacements.
  for (const slot of slots) gl.deleteTexture(slot.texture)
  assert.equal(new Set(deleted).size, next)
})

test('failed allocation preserves the old texture and retries; disposal does not overwrite another hook', () => {
  const old = {}
  let replacement = null
  let deletes = 0
  let draws = 0
  const gl = {
    createTexture: () => replacement,
    activeTexture() {},
    bindTexture() {},
    texParameteri() {},
    deleteTexture() {
      deletes++
    }
  }
  const slot = { texture: old, version: 2 }
  const glyph = {
    _atlasTextures: [slot],
    _atlas: { pages: [{ canvas: { width: 512, height: 512 } }] },
    render() {
      draws++
    }
  }
  const adapter = registerWebglTextureSlots(gl, glyph)
  glyph.render()
  assert.equal(slot.texture, old)
  assert.equal(slot.version, 2)
  assert.equal(deletes, 0)
  replacement = {}
  glyph.render()
  assert.equal(slot.texture, replacement)
  assert.equal(deletes, 1)
  assert.equal(draws, 2)
  const otherHook = () => {}
  glyph.render = otherHook
  adapter.dispose()
  assert.equal(glyph.render, otherHook)
  assert.throws(() => registerWebglTextureSlots(gl, {}), /Unsupported/)
})
