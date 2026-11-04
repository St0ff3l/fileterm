import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/app/terminal-renderer-preference.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)

test('terminal renderer is available on native desktop platforms only', () => {
  assert.equal(exports.getTerminalRendererPreference(), 'webgl')
  for (const platform of ['win32', 'darwin', 'linux']) {
    assert.equal(exports.supportsTerminalRenderer(platform), true)
  }
  for (const platform of [undefined, 'browser', 'freebsd']) {
    assert.equal(exports.supportsTerminalRenderer(platform), false)
  }
})
