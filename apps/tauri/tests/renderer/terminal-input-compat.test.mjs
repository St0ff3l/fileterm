import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const code = ts.transpileModule(
  readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-input-compat.ts', import.meta.url), 'utf8'),
  { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
).outputText

function fixture() {
  const exports = {}
  const window = new EventTarget()
  const textarea = new EventTarget()
  textarea.value = ''
  let now = 1000
  vm.runInNewContext(code, { exports, window, Date: { now: () => now } })
  const ref = (current) => ({ current })
  const written = []
  const options = {
    terminalTextarea: textarea,
    isActiveRef: ref(true),
    connectedRef: ref(true),
    serialTransferBusyRef: ref(false),
    imeCompositionActiveRef: ref(false),
    imeCompositionTextRef: ref(''),
    imeCompositionEndedAtRef: ref(0),
    imeCompositionDataForwardedRef: ref(false),
    clearEphemeralHighlight() {},
    setContextMenu() {},
    writeTerminalInput: (data) => written.push(data)
  }
  const compatibility = exports.createTerminalInputCompatibility(options)
  const composition = (type, data = '') => textarea.dispatchEvent(Object.assign(new Event(type), { data }))
  const input = (data) => ({
    target: textarea,
    inputType: 'insertText',
    data,
    isComposing: false,
    preventDefault() {
      this.prevented = true
    },
    stopImmediatePropagation() {}
  })
  return { compatibility, options, written, composition, input, advance: (ms) => (now += ms) }
}

test('composition commits once and ordinary repeated input survives after the IME window', () => {
  const { compatibility, composition, advance } = fixture()
  composition('compositionstart')
  composition('compositionupdate', '中文')
  assert.equal(compatibility.filterCompositionData('中'), null)
  composition('compositionend', '文')
  assert.equal(compatibility.filterCompositionData('中文'), '中文', 'latest compositionupdate owns the commit')
  assert.equal(compatibility.filterCompositionData('中文'), null)
  assert.equal(compatibility.filterCompositionData(' '), null, 'candidate confirmation is not shell input')
  advance(76)
  assert.equal(compatibility.filterCompositionData('中文'), '中文')
  assert.equal(compatibility.filterCompositionData('中文'), '中文')
  compatibility.dispose()
})

test('native input before keydown is forwarded once, while xterm-owned input is not resent', () => {
  const { compatibility, written, input } = fixture()
  const event = input('a')
  compatibility.onTerminalInputCapture(event)
  compatibility.onTerminalInputAfterXterm(event)
  assert.deepEqual(written, ['a'])
  const keydown = {
    type: 'keydown',
    key: 'a',
    code: 'KeyA',
    preventDefault() {},
    stopPropagation() {}
  }
  assert.equal(compatibility.handleKeyEvent(keydown), false, 'the delayed physical key must not echo twice')
  assert.equal(compatibility.handleKeyEvent({ ...keydown, key: 'b', code: 'KeyB' }), null)
  compatibility.markDataEmitted('b')
  const owned = input('b')
  compatibility.onTerminalInputCapture(owned)
  compatibility.onTerminalInputAfterXterm(owned)
  assert.equal(owned.prevented, true)
  assert.deepEqual(written, ['a'])
  compatibility.dispose()
})

test('Shift symbols keep their layout result and dispose stops composition tracking', () => {
  const { compatibility, options, written, input, composition } = fixture()
  const key = {
    type: 'keydown',
    key: '¥',
    code: 'Digit3',
    shiftKey: true,
    preventDefault() {},
    stopPropagation() {}
  }
  assert.equal(compatibility.handleKeyEvent(key), false)
  const duplicate = input('¥')
  compatibility.onTerminalInputCapture(duplicate)
  compatibility.onTerminalInputAfterXterm(duplicate)
  assert.deepEqual(written, ['¥'])
  assert.equal(duplicate.prevented, true)
  compatibility.dispose()
  compatibility.dispose()
  composition('compositionstart')
  assert.equal(options.imeCompositionActiveRef.current, false)
})
