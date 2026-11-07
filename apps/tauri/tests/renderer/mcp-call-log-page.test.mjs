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

test('refreshing an empty audit page preserves the empty-state markup', async () => {
  const React = await import('react')
  const jsx = await import('react/jsx-runtime')
  const { renderToStaticMarkup } = await import('react-dom/server')
  const source = readFileSync(
    new URL('../../src/renderer/features/workspace/mcp-call-log-page.tsx', import.meta.url),
    'utf8'
  )
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }
  }).outputText
  function renderEmptyState(page, loading) {
    const exports = {}
    const state = [page, loading, null, null, false, null]
    const dependencies = {
      react: {
        useState: () => [state.shift(), () => {}],
        useRef: () => ({ current: null }),
        useCallback: (callback) => callback,
        useEffect() {}
      },
      'react/jsx-runtime': jsx,
      '../../i18n': { t: new Proxy({}, { get: (_, key) => key }) },
      '../common/app-icon': { AppIcon: () => React.createElement('svg') },
      '../common/vertical-scrollbar': { VerticalScrollbar: () => null }
    }
    vm.runInNewContext(output, { exports, require: (name) => dependencies[name] ?? {} })
    const tree = exports.McpCallLogPage({ onTotalChange() {} })
    function findEmpty(node) {
      if (!node || typeof node !== 'object') return null
      if (node.props?.className === 'background-session-empty') return node
      return React.Children.toArray(node.props?.children).map(findEmpty).find(Boolean)
    }
    return renderToStaticMarkup(findEmpty(tree))
  }
  const emptyPage = { items: [], total: 0, hasMore: false, nextCursor: null }
  const ready = renderEmptyState(emptyPage, false)
  assert.match(ready, /mcpCallRecordsEmptyHint/)
  assert.equal(renderEmptyState(emptyPage, true), ready)
  const initial = renderEmptyState(null, true)
  assert.match(initial, /mcpCallRecordsLoading/)
  assert.doesNotMatch(initial, /mcpCallRecordsEmptyHint/)
  assert.equal(renderEmptyState(null, false), initial)
})
