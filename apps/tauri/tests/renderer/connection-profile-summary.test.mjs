import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as jsxRuntime from 'react/jsx-runtime'
import { DEFAULT_OVERVIEW_SECTION_ORDER } from '@fileterm/core'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/app/connection-profile-summary.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports }
)
const summary = exports.connectionProfileSummary

test('serial summaries include the device, baud rate and every supported parity tag', () => {
  for (const [parity, tag] of Object.entries({ none: 'N', odd: 'O', even: 'E', mark: 'M', space: 'S' })) {
    assert.equal(
      summary({ type: 'serial', devicePath: 'COM3', baudRate: 9600, dataBits: 7, parity, stopBits: 2 }),
      `COM3 · 9600 baud · 7${tag}2`
    )
  }
  assert.equal(summary({ type: 'serial', devicePath: '/dev/cu.usbserial' }), '/dev/cu.usbserial · 115200 baud · 8N1')
})

test('network connection summaries retain their user and host', () => {
  for (const type of ['ssh', 'ftp', 'telnet']) {
    assert.equal(summary({ type, username: 'lab', host: 'example.test' }), 'lab@example.test')
  }
})

function loadComponent(path, dependencies) {
  const moduleExports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    {
      exports: moduleExports,
      require(name) {
        if (name === 'react/jsx-runtime') return jsxRuntime
        if (name.endsWith('.css')) return {}
        assert.ok(name in dependencies, `Unexpected dependency: ${name}`)
        return dependencies[name]
      }
    }
  )
  return moduleExports
}

test('recent and all-connection cards display serial framing instead of network credentials', () => {
  const tree = loadComponent('../../src/renderer/features/connections/connection-tree.ts', {})
  const { OverviewPage } = loadComponent('../../src/renderer/features/workspace/overview-page.tsx', {
    '@fileterm/core': { DEFAULT_OVERVIEW_SECTION_ORDER },
    '../../i18n': { t: new Proxy({}, { get: (_target, key) => key }) },
    '../../app/connection-profile-summary': exports,
    '../connections/connection-tree': tree,
    '../../components/common/material-icon/material-icon': { MaterialIcon: () => null }
  })
  const profile = {
    id: 'serial-1',
    name: 'Console',
    type: 'serial',
    devicePath: 'COM3',
    baudRate: 9600,
    dataBits: 7,
    parity: 'mark',
    stopBits: 2,
    lastUsedAt: 1,
    username: 'ignored-user',
    host: 'ignored-host'
  }
  const html = renderToStaticMarkup(
    createElement(OverviewPage, {
      profiles: [profile],
      sectionOrder: ['recent', 'allConnections'],
      showRecent: true,
      showAllConnections: true,
      showStats: false,
      showQuickActions: false
    })
  )
  assert.equal((html.match(/>COM3 · 9600 baud · 7M2<\/span>/g) ?? []).length, 2)
  assert.doesNotMatch(html, /ignored-user@ignored-host|undefined@undefined/)
})
