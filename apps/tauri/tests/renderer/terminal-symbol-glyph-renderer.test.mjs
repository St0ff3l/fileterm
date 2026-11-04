import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture(text, fitted = false, cellWidth, lineCodes) {
  let layoutReads = 0
  class Element {
    textContent = text
    children = [{ textContent: text, classList: { contains: () => fitted } }]
  }
  const bufferLine = lineCodes
    ? { length: lineCodes.length, getCell: (column) => ({ getCode: () => lineCodes[column] ?? 0x20 }) }
    : undefined
  const rows = {
    children: [new Element()],
    ownerDocument: {
      getElementById: () => null,
      head: { append() {} },
      defaultView: {
        HTMLElement: Element,
        MutationObserver: class {
          observe() {}
          takeRecords() {}
          disconnect() {}
        }
      },
      createElement: () => ({ getContext: () => ({}) })
    },
    getBoundingClientRect: () => {
      layoutReads++
      return { width: 800 }
    }
  }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(
        new URL('../../src/renderer/xterm-adapters/terminal-symbol-glyph-renderer.ts', import.meta.url),
        'utf8'
      ),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      require: () => ({
        trackRendererWork: (_phase, work) => work,
        isBoxDrawingGlyph: (code) => code >= 0x2500 && code <= 0x257f
      })
    }
  )
  const registration = exports.registerTerminalSymbolGlyphRenderer({
    cols: 80,
    options: {},
    _core: { _renderService: { dimensions: { css: { cell: { width: cellWidth } } } } },
    element: { querySelector: () => rows },
    buffer: { active: { getNullCell: () => ({}), getLine: () => bufferLine } },
    onRender: () => ({ dispose() {} })
  })
  registration.dispose()
  return layoutReads
}

test('graphical-only rows do not force layout in the font-symbol adapter', () => {
  for (const text of ['─'.repeat(200), '┼│━═▁█', '┃ result: ok ┃', '┃中文┃', 'plain ASCII!']) {
    assert.equal(fixture(text), 0, text)
  }
})

test('fallback symbols and CJK punctuation still enter the fitting path', () => {
  for (const text of ['✻', '┃中文:┃', '┃✓┃', '中文，。']) assert.equal(fixture(text), 1, text)
})

test('already fitted rows skip geometry on repeated render notifications', () => {
  for (const text of ['✻', '中文，。', '┃✓┃']) assert.equal(fixture(text, true), 0, text)
})

test('the calibrated xterm CSS grid avoids another geometry read', () => {
  assert.equal(fixture('✻', false, 10), 0)
})

test('ASCII-only buffer rows skip the DOM text walk even when the DOM text has a symbol', () => {
  const ascii = Array.from({ length: 80 }, () => 0x41)
  assert.equal(fixture('✻', false, undefined, ascii), 0)
})

test('rows whose buffer holds a non-ASCII codepoint still enter the fitting path', () => {
  const codes = Array.from({ length: 80 }, () => 0x41)
  codes[3] = 0x273b
  assert.equal(fixture('✻', false, undefined, codes), 1)
})

test('an unavailable buffer line falls back to the DOM check instead of skipping', () => {
  assert.equal(fixture('✻', false, undefined, undefined), 1)
})
