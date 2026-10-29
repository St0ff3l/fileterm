import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as jsx from 'react/jsx-runtime'
import ts from 'typescript'

test('error fallback uses the shared dialog frame and shows read-only details', () => {
  const exports = {}
  const dependencies = {
    react: React,
    'react/jsx-runtime': jsx,
    '../../components/common/button/button': {
      Button: ({ children }) => React.createElement('button', null, children)
    },
    '../../components/common/dialog/form-dialog.css': {},
    '../../i18n': {
      t: { errorBoundaryTitle: 'Error', errorBoundaryDescription: 'Reload the app', errorBoundaryReload: 'Reload' }
    },
    '../../lib/renderer-error-log': {},
    './error-boundary.css': {}
  }
  const source = readFileSync(new URL('../src/renderer/features/common/error-boundary.tsx', import.meta.url), 'utf8')
  vm.runInNewContext(
    ts.transpileModule(source, {
      compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports, require: (name) => dependencies[name] }
  )
  const boundary = new exports.ErrorBoundary({ children: null })
  boundary.state = { error: new Error('Example failure') }
  const html = renderToStaticMarkup(boundary.render())
  assert.match(html, /modal-card form-dialog error-boundary-dialog/)
  assert.match(html, /<pre>Example failure<\/pre>/)
  assert.match(html, /<button[^>]*>Reload<\/button>/)
  assert.doesNotMatch(html, /<input|<textarea/)
})
