import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'
import { Unicode11Addon } from '@xterm/addon-unicode11'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-emoji-widths.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)
const { registerTerminalEmojiWidths } = exports

function createUnicodeService() {
  const unicode = {
    register(provider) {
      this.provider = provider
    }
  }
  new Unicode11Addon().activate({ unicode })
  return { charProperties: (codepoint, preceding) => unicode.provider.charProperties(codepoint, preceding) }
}

function stringCellWidth(unicode, text) {
  let width = 0
  let preceding = 0
  for (const character of text) {
    const properties = unicode.charProperties(character.codePointAt(0), preceding)
    const currentWidth = (properties >>> 1) & 0x3
    width += currentWidth - (properties & 1 ? (preceding >>> 1) & 0x3 : 0)
    preceding = properties
  }
  return width
}

test('emoji presentation and grapheme sequences occupy the expected terminal cells', () => {
  const unicode = createUnicodeService()
  const original = unicode.charProperties
  const dispose = registerTerminalEmojiWidths({ _core: { unicodeService: unicode } }).dispose

  for (const glyph of ['⚠️', '✔️', '✖️', '©️', '‼️', '1️⃣', '1⃣', '#️⃣', '🇨🇳', '👍🏽', '👩🏽‍🚀', '👨‍👩‍👧‍👦']) {
    assert.equal(stringCellWidth(unicode, glyph), 2, `${glyph} should occupy two cells`)
  }
  assert.equal(stringCellWidth(unicode, '🇨🇳🇺🇸'), 4, 'two flags should occupy four cells')
  assert.equal(stringCellWidth(unicode, '✅🚨'), 4, 'default emoji widths stay two cells each')
  assert.equal(stringCellWidth(unicode, '⚠︎'), 1, 'text presentation stays one cell')
  for (const symbol of ['✻', '※', '①', '•', '○', '■', '§', '±', '×']) {
    assert.equal(stringCellWidth(unicode, symbol), 1, `${symbol} retains the producer's one-column contract`)
  }
  assert.equal(stringCellWidth(unicode, 'A️'), 1, 'an invalid emoji variation sequence is not widened')

  dispose()
  assert.equal(unicode.charProperties, original)
})
