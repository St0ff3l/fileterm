import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/connections/ssh-config-host-field.tsx', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }
  ).outputText,
  { exports, require: () => ({}) }
)

test('native SSH config host picker is macOS-only', () => {
  assert.equal(exports.usesNativeSshConfigHostPicker('darwin'), true)
  assert.equal(exports.usesNativeSshConfigHostPicker('win32'), false)
  assert.equal(exports.usesNativeSshConfigHostPicker('linux'), false)
  assert.equal(exports.usesNativeSshConfigHostPicker(undefined), false)
})
