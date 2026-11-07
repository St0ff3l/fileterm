import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function loadFormatters() {
  const exports = {}
  const source = readFileSync(
    new URL('../../src/renderer/features/workspace/mcp-call-log-page.tsx', import.meta.url),
    'utf8'
  )
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }
  }).outputText
  vm.runInNewContext(output + '\nexports.formatters = { approvalLabel, resultLabel, resultClass, summarizeParams };', {
    exports,
    require: (name) => (name === '../../i18n' ? { t: new Proxy({}, { get: (_, key) => key }) } : {})
  })
  return exports.formatters
}

test('audit approval and execution labels preserve backend decisions', () => {
  const { approvalLabel, resultLabel, resultClass } = loadFormatters()
  assert.equal(approvalLabel('user'), 'mcpCallApprovalUser')
  assert.equal(approvalLabel('rejected'), 'mcpCallApprovalRejected')
  assert.equal(approvalLabel('timed-out'), 'mcpCallApprovalTimedOut')
  assert.equal(approvalLabel('legacy-unknown'), 'mcpCallLegacyUnknown')
  assert.equal(resultLabel({ status: 'accepted', ok: true }), 'mcpCallResultAccepted')
  assert.equal(resultLabel({ status: 'started', ok: false }), 'mcpCallResultStarted')
  assert.equal(resultLabel({ status: 'command-failed', callOk: true }), 'mcpCallResultCommandFailed')
  assert.equal(resultLabel({ status: 'legacy-unknown', ok: true }), 'mcpCallLegacyUnknown')
  assert.equal(resultClass({ status: 'started', ok: false }), '')
  assert.equal(resultClass({ status: 'command-failed' }), 'is-failed')
  assert.equal(resultClass({ status: 'outcome-uncertain' }), 'is-uncertain')
})

test('audit summary uses resolved target and path metadata', () => {
  const { summarizeParams } = loadFormatters()
  assert.equal(
    summarizeParams({
      target: { host: 'example.test', port: 22, profileId: 'profile-1' },
      params: { tab_id: 'tab-1', path: '/etc/app.conf', command: 'omitted raw shell text' }
    }),
    'example.test:22 · /etc/app.conf'
  )
  assert.equal(summarizeParams({ target: null, params: { tab_id: 'tab-1' } }), 'tab-1')
  assert.equal(summarizeParams({ target: null, params: {} }), '')
})
