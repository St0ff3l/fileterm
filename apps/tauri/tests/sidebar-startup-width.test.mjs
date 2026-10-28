import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(path, context = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      fileName: path,
      compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    { exports, ...context }
  )
  return exports
}

const utils = load('../src/renderer/app/app-shell-utils.ts')

function sidebarFixture() {
  let widths = {}
  const { useWorkspaceSidebarWidth } = load('../src/renderer/hooks/use-workspace-sidebar-width.ts', {
    require(name) {
      if (name.endsWith('/app-shell-utils')) return utils
      assert.equal(name, 'react')
      return {
        useCallback: (callback) => callback,
        useEffect: (effect) => effect(),
        useState: () => [
          widths,
          (update) => {
            widths = update(widths)
          }
        ]
      }
    }
  })
  return (id, sessions = [{ id: 'a' }, { id: 'b' }], locals = [{ id: 'home' }]) =>
    useWorkspaceSidebarWidth(id, sessions, locals)
}

test('tabs and windows start at the same default and retain independent custom widths', () => {
  const render = sidebarFixture()
  const otherWindow = sidebarFixture()
  for (const id of ['a', 'b', 'home']) assert.equal(render(id).activeSidebarWidth, 214)
  render('a').setActiveSidebarWidth(320)
  render('b').setActiveSidebarWidth(280)
  render('home').setActiveSidebarWidth(250)
  assert.equal(render('a').activeSidebarWidth, 320)
  assert.equal(render('b').activeSidebarWidth, 280)
  assert.equal(render('home').activeSidebarWidth, 250)
  assert.equal(otherWindow('a').activeSidebarWidth, 214)
  render(null).setActiveSidebarWidth(360)
  assert.equal(render('a').activeSidebarWidth, 320)
  render(null, [{ id: 'b' }], [{ id: 'home' }])
  assert.equal(render('a').activeSidebarWidth, 214, 'closed tabs must release their width')
  render(null, [], [])
  assert.equal(render('home').activeSidebarWidth, 214, 'closed home tabs must release their width')
})

function fixture(initialViewportWidth, initialSidebarWidth = utils.DEFAULT_SIDEBAR_WIDTH) {
  const listeners = new Map()
  const states = []
  const effects = []
  let stateIndex = 0
  let effectIndex = 0
  let preferredWidth = initialSidebarWidth
  const window = {
    innerWidth: initialViewportWidth,
    addEventListener(name, callback) {
      if (!listeners.has(name)) listeners.set(name, new Set())
      listeners.get(name).add(callback)
    },
    removeEventListener: (name, callback) => listeners.get(name)?.delete(callback)
  }
  const hook = load('../src/renderer/hooks/use-app-resize.ts', {
    window,
    require(name) {
      if (name === '../app/app-shell-utils') return utils
      assert.equal(name, 'react')
      return {
        useCallback: (callback) => callback,
        useState(initialValue) {
          const index = stateIndex++
          if (!(index in states)) states[index] = typeof initialValue === 'function' ? initialValue() : initialValue
          return [
            states[index],
            (update) => {
              states[index] = typeof update === 'function' ? update(states[index]) : update
            }
          ]
        },
        useEffect(effect, dependencies) {
          const index = effectIndex++
          const previous = effects[index]
          if (previous && dependencies.every((value, i) => Object.is(value, previous.dependencies[i]))) return
          previous?.cleanup?.()
          effects[index] = { dependencies, cleanup: effect() }
        }
      }
    }
  })
  const render = (home = true) => {
    stateIndex = effectIndex = 0
    return hook.useAppResize({
      isHomeWorkspaceVisible: home,
      activeSidebarWidth: preferredWidth,
      workspace: { tabs: [] }
    })
  }
  return {
    render,
    resize(width) {
      window.innerWidth = width
      listeners.get('resize')?.forEach((callback) => callback())
      return render()
    },
    dispose: () => effects.forEach((effect) => effect.cleanup?.()),
    get preferredWidth() {
      return preferredWidth
    },
    get resizeListenerCount() {
      return listeners.get('resize')?.size ?? 0
    }
  }
}

test('a temporary startup viewport does not permanently shrink the default sidebar', () => {
  for (const width of [1, 800, 1120]) {
    const app = fixture(width)
    const initial = app.render()
    assert.equal(app.preferredWidth, 214)
    assert.equal(initial.expandedSidebarWidth, 190)
    assert.equal(app.resize(1280).expandedSidebarWidth, 214)
    assert.equal(app.preferredWidth, 214)
    app.dispose()
    assert.equal(app.resizeListenerCount, 0)
  }
})

test('the default width fits the supported minimum and default window sizes', () => {
  for (const width of [1150, 1280]) {
    const app = fixture(width)
    assert.equal(app.render().expandedSidebarWidth, 214)
    app.dispose()
  }
})

test('narrow windows temporarily constrain a manually widened sidebar', () => {
  const app = fixture(1280, 320)
  assert.equal(app.render().expandedSidebarWidth, 320)
  assert.equal(app.resize(1150).expandedSidebarWidth, 220)
  assert.equal(app.preferredWidth, 320)
  assert.equal(app.resize(1280).expandedSidebarWidth, 320)
  app.dispose()
})

test('a session keeps its manually widened sidebar even when the home content needs more space', () => {
  const app = fixture(1150, 320)
  assert.equal(app.render(false).expandedSidebarWidth, 320)
  assert.equal(app.render(true).expandedSidebarWidth, 220)
  assert.equal(app.preferredWidth, 320)
  app.dispose()
})

test('a manually narrowed sidebar stays narrow when the window grows', () => {
  const app = fixture(1120, 190)
  assert.equal(app.render().expandedSidebarWidth, 190)
  assert.equal(app.resize(1280).expandedSidebarWidth, 190)
  assert.equal(app.preferredWidth, 190)
  app.dispose()
})

function renderWorkspace({
  home = false,
  hostGroups = false,
  local = false,
  copilot = false,
  transfers = false,
  width = 214
} = {}) {
  const { AppMainWorkspace } = load('../src/renderer/features/layout/app-main-workspace.tsx', {
    require(name) {
      if (name.endsWith('.css')) return {}
      if (name === 'react/jsx-runtime') {
        const jsx = (type, props) => ({ type, props, children: [props.children].flat() })
        return { jsx, jsxs: jsx }
      }
      if (name === 'react') {
        return { useState: (value) => [value, () => {}], useEffect() {} }
      }
      if (name.endsWith('/use-host-navigation')) return { useHostNavigation: () => ({ groups: [] }) }
      if (name.endsWith('/use-workspace-tab-shortcuts')) return { useWorkspaceTabShortcuts() {} }
      if (name.endsWith('/shared-monitoring')) return { resolveMonitoringSource: () => ({ peers: [] }) }
      if (name.endsWith('/app-shell-utils')) return utils
      if (name.endsWith('/i18n')) return { t: {} }
      return {}
    }
  })
  return AppMainWorkspace({
    model: {
      shell: {
        workspace: { profiles: [], sessions: {} },
        sessionSecurity: {},
        sidebarWidth: 320,
        tabBarLayout: hostGroups ? 'host-groups' : 'horizontal'
      },
      workspace: {
        isHomeWorkspaceVisible: home,
        isLocalTerminalWorkspace: local,
        shouldShowAiCopilot: copilot,
        activeLocalTab: transfers ? { kind: 'transfers' } : home ? { kind: 'home' } : null,
        visibleWorkspaceTabs: [],
        localTabs: [],
        activeSidebarMetricOrder: [],
        activeSidebarMetrics: [],
        activeTerminalDockSendState: {},
        activeSidebarWidth: width
      },
      data: {},
      resize: { expandedSidebarWidth: home ? Math.min(width, 220) : width }
    }
  }).children[0]
}

test('titlebar brand width stays fixed when home, session and host sidebars resize', () => {
  for (const home of [false, true]) {
    for (const width of [190, 214, 320]) {
      const shell = renderWorkspace({ home, width })
      assert.equal(shell.props.style['--sidebar-width'], `${home ? Math.min(width, 220) : width}px`)
      assert.equal(shell.props.style['--brand-width'], '214px')
    }
  }
  const grouped = renderWorkspace({ hostGroups: true, width: 300 })
  assert.equal(grouped.props.style['--sidebar-width'], '300px')
  assert.equal(grouped.props.style['--host-rail-width'], '320px')
  assert.equal(grouped.props.style['--brand-width'], '214px')
})

test('remote, transfer and local tabs reuse the same visible terminal edge frame', () => {
  for (const options of [
    {},
    { home: true },
    { hostGroups: true },
    { local: true },
    { copilot: true },
    { transfers: true }
  ]) {
    const frame = renderWorkspace(options).children.find((child) =>
      child?.props?.className?.startsWith('terminal-right-frame')
    )
    assert.ok(frame)
    assert.equal(frame.props.className.includes('is-visible'), !options.home && !options.hostGroups && !options.copilot)
    assert.equal(frame.props['aria-hidden'], 'true')
    const shell = renderWorkspace(options)
    assert.equal(
      shell.props.className.includes('has-workspace-left-frame'),
      !options.hostGroups && Boolean(options.local || options.transfers)
    )
    assert.ok(shell.children.some((child) => child?.props?.className === 'workspace-left-frame'))
  }
})
