import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const code = ts.transpileModule(
  readFileSync(new URL('../../src/renderer/features/system/monitoring-toggle.tsx', import.meta.url), 'utf8'),
  { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }
).outputText

function render(session, available = true) {
  const exports = {}
  const requests = []
  const jsx = (type, props) => ({ type, props })
  vm.runInNewContext(code, {
    exports,
    window: { fileterm: available ? { setMonitoringEnabled: (...args) => requests.push(args) } : undefined },
    require: (name) => {
      if (name === 'react')
        return { useState: (value) => [value, () => {}], useEffect() {}, useRef: (current) => ({ current }) }
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' }
      if (name === '../../i18n')
        return { t: { monitoringStart: 'Start', monitoringStop: 'Stop', monitoringSshDisconnected: 'Disconnected' } }
      if (name.includes('button/button')) return { Button: 'Button' }
      return {}
    }
  })
  const result = exports.MonitoringToggle({ session, tabId: 'tab', onSnapshot() {} })
  return { button: result?.props.children[0], requests }
}

const session = (connected, phase = 'healthy', reason) => ({
  connected,
  capabilities: { resourceMonitoring: true },
  monitoring: { phase, reason, generation: 1 }
})

test('monitoring power control stays visible but disabled throughout SSH disconnection', () => {
  for (const current of [
    session(false),
    session(false, 'disconnected', 'ssh-disconnected'),
    session(true, 'disconnected', 'ssh-disconnected')
  ]) {
    const { button, requests } = render(current)
    assert.ok(button)
    assert.equal(button.props.disabled, true)
    assert.equal(button.props.title, 'Disconnected')
    button.props.onClick()
    assert.deepEqual(requests, [])
  }
})

test('the same control is usable again after reconnect and reflects stopped state', () => {
  for (const phase of ['healthy', 'stopped']) {
    const { button } = render(session(true, phase))
    assert.equal(button.props.disabled, false)
    assert.equal(button.props.title, phase === 'stopped' ? 'Start' : 'Stop')
  }
})

test('unsupported sessions stay excluded while a missing API only disables the control', () => {
  assert.equal(render(session(true, 'unsupported')).button, undefined)
  assert.equal(render({ ...session(true), capabilities: { resourceMonitoring: false } }).button, undefined)
  assert.equal(render(session(true), false).button.props.disabled, true)
})
