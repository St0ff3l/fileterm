import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-webgl-text-baseline.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)

test('baseline offsets follow bold/italic flags, cache layout, remeasure loaded fonts, and release hooks', () => {
  let created = 0,
    attached = 0,
    queried = 0,
    shift = 0
  const listeners = new Set()
  const context = { measureText: () => ({ alphabeticBaseline: 4 }) }
  const document = {
    defaultView: { devicePixelRatio: 1.25 },
    fonts: {
      addEventListener: (event, listener) => listeners.add(listener),
      removeEventListener: (event, listener) => listeners.delete(listener)
    },
    body: {
      append(node) {
        node.attached = true
        attached++
      }
    },
    createTextNode: () => ({}),
    createElement() {
      created++
      return {
        style: {},
        setAttribute() {},
        append() {},
        getContext: () => context,
        getBoundingClientRect: () => ({ top: 14.4 + (queried++ % 4) * 0.8 + shift }),
        remove() {
          if (this.attached) attached--
        }
      }
    }
  }
  const terminal = {
    cols: 1,
    element: { ownerDocument: document },
    options: { fontFamily: 'Consolas', fontSize: 14, fontWeight: 400, fontWeightBold: 700 }
  }
  const attributes = new Float32Array(11)
  const update = () => {
    attributes[1] = 5
  }
  const glyph = { _vertices: { attributes }, updateCell: update }
  const renderer = {
    _glyphRenderer: { value: glyph },
    dimensions: { css: { cell: { height: 18.4 } }, device: { char: { height: 22, top: 0 } } }
  }
  const adapter = exports.createWebglTextBaseline(terminal)
  adapter.refresh(renderer)
  const measuredElements = created
  const variants = [
    [0, 0],
    [0, 0x08000000],
    [0x04000000, 0],
    [0x04000000, 0x08000000]
  ]
  variants.forEach(([bg, fg], style) => {
    glyph.updateCell(0, 0, 72, bg, fg)
    assert.equal(attributes[1], 5 + style)
  })
  for (let i = 0; i < 100; i++) adapter.refresh(renderer)
  assert.equal(created, measuredElements, 'stable draws must not perform DOM layout or create canvases')
  shift = 0.8
  for (const listener of listeners) listener()
  adapter.refresh(renderer)
  glyph.updateCell(0, 0, 72, 0, 0)
  assert.equal(attributes[1], 6)
  glyph.updateCell(0, 0, 0x2500, 0, 0)
  assert.equal(attributes[1], 5, 'cell geometry is independent of text baselines')
  glyph.updateCell(0, 0, 0x4e2d, 0, 0)
  assert.equal(attributes[1], 5, 'retain fallback placement until separately calibrated')
  const replacement = { _vertices: { attributes }, updateCell: update }
  renderer._glyphRenderer.value = replacement
  adapter.refresh(renderer)
  assert.equal(glyph.updateCell, update, 'context restoration releases the old renderer hook')
  adapter.dispose()
  assert.equal(replacement.updateCell, update)
  assert.equal(listeners.size, 0)
  assert.equal(attached, 0)
})
