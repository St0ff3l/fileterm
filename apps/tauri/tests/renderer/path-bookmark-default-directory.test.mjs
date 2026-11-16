import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

test('the bookmarks provider saves the current path to the matching connection field', async () => {
  const calls = []
  const exports = {}
  const source = readFileSync(
    new URL('../../src/renderer/features/files/path-bookmark-provider.tsx', import.meta.url),
    'utf8'
  )
  vm.runInNewContext(
    ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        jsx: ts.JsxEmit.ReactJSX
      }
    }).outputText,
    {
      exports,
      window: {
        fileterm: {
          setProfileDefaultDirectory: async (...args) => calls.push(args),
          updatePathBookmark: async () => []
        }
      },
      require: (name) => {
        if (name === 'react') return { createContext: (value) => ({ value }), useState: () => [null, () => {}] }
        if (name === 'react/jsx-runtime')
          return { jsx: (type, props) => ({ type, props }), jsxs: (type, props) => ({ type, props }) }
        if (name === '../../i18n') return { t: {} }
        if (name === '../common/confirm-action-dialog') return { ConfirmActionDialog: () => null }
        throw new Error(`Unexpected import ${name}`)
      }
    }
  )

  const tree = exports.PathBookmarkProvider({ profileId: 'profile-1', children: null })
  const setter = tree.props.children.props.value
  await setter('local', 'C:/projects')
  await setter('remote', '/srv/www')
  assert.deepEqual(calls, [
    ['profile-1', 'local', 'C:/projects'],
    ['profile-1', 'remote', '/srv/www']
  ])
})
