import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function loadConstantsModule() {
  const source = readFileSync(
    new URL('../../src/renderer/features/settings/settings-modal/constants.tsx', import.meta.url),
    'utf8'
  )
  const exports = {}
  const modules = {
    'react/jsx-runtime': { jsx: () => null, jsxs: () => null },
    react: { useEffect: () => undefined, useState: (initial) => [initial, () => undefined] },
    '@fileterm/core': {},
    '../../../app/theme-config': {},
    '../../../i18n': {},
    '../../../components/common/material-icon/material-icon': {},
    '../../common/app-icon': {},
    '../../../../bridge/web-login': {}
  }
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
      require: (name) => modules[name] ?? {}
    }
  )
  return exports
}

const controllerSource = readFileSync(
  new URL('../../src/renderer/features/settings/settings-modal/controller/index.ts', import.meta.url),
  'utf8'
)
const settingsModalSource = readFileSync(
  new URL('../../src/renderer/features/settings/settings-modal/index.tsx', import.meta.url),
  'utf8'
)
const mainSource = readFileSync(new URL('../../src/renderer/main.tsx', import.meta.url), 'utf8')
const entrypointSource = readFileSync(new URL('../../web/entrypoint.sh', import.meta.url), 'utf8')
const webBinarySource = readFileSync(new URL('../../src-tauri/src/bin/fileterm_web.rs', import.meta.url), 'utf8')

test('settings sidebar removes management tools tab and restricts web-login to Docker web runtime', () => {
  const { SETTINGS_SIDEBAR_ITEMS, SETTINGS_TAB_SEARCH_TERMS, isWebLoginTabAvailable } = loadConstantsModule()

  assert.equal(
    SETTINGS_SIDEBAR_ITEMS.some((item) => item.tab === 'tools'),
    false
  )
  assert.equal('tools' in SETTINGS_TAB_SEARCH_TERMS, false)
  assert.equal(settingsModalSource.includes('ToolsSettingsPanel'), false)

  // Desktop / installer builds must never expose the web-login tab, even if a status were present.
  assert.equal(isWebLoginTabAvailable(true, null), false)
  assert.equal(isWebLoginTabAvailable(true, { mode: 'managed', username: 'admin' }), false)
  assert.equal(isWebLoginTabAvailable(true, { mode: 'deployment' }), false)

  // Non-Docker standalone web runtime (webLoginStatus === null) must not expose web-login.
  assert.equal(isWebLoginTabAvailable(false, null), false)

  // Only Docker web runtime (non-desktop + non-null webLoginStatus) exposes web-login.
  assert.equal(isWebLoginTabAvailable(false, { mode: 'managed', username: 'operator' }), true)
  assert.equal(isWebLoginTabAvailable(false, { mode: 'deployment' }), true)
  assert.equal(isWebLoginTabAvailable(false, { mode: 'setup', username: '' }), true)

  assert.match(controllerSource, /isWebLoginTabAvailable\(/)
  assert.match(settingsModalSource, /!isDesktopRuntime\(\) && !desktopApi\?\.isDesktop \? getWebLoginStatus\(\) : null/)
  assert.match(mainSource, /isDesktop \|\| api\.isDesktop \? null : getWebLoginStatus\(\)/)
  assert.match(entrypointSource, /export FILETERM_WEB_DOCKER_LOGIN=1/)
  assert.match(webBinarySource, /FILETERM_WEB_DOCKER_LOGIN/)
})
