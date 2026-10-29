import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

test('layout observes sidebar replacement and collapse without terminal subtree mutations', () => {
  const observers = []
  class Observer {
    constructor(callback) {
      this.callback = callback
      observers.push(this)
    }
    observe(target, options) {
      this.target = target
      this.options = options
    }
    disconnect() {
      this.target = null
    }
  }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(
        new URL('../src/renderer/features/workspace/file-panel-layout-observer.ts', import.meta.url),
        'utf8'
      ),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      MutationObserver: Observer,
      require: () => ({ FILE_PANEL_SNAP_TARGET_SELECTORS: { header: '[data-header]' } })
    }
  )
  const card = () => ({})
  const sidebar = () => ({
    collapsed: false,
    targets: [card(), card()],
    classList: { contains: () => currentSidebar.collapsed },
    querySelectorAll: () => currentSidebar.targets
  })
  let currentSidebar = sidebar()
  const root = { querySelector: () => currentSidebar }
  const observed = new Set()
  let layouts = 0
  const dispose = exports.observeFilePanelLayout(
    root,
    {
      observe: (node) => observed.add(node),
      unobserve: (node) => observed.delete(node)
    },
    () => layouts++
  )
  const shellObserver = observers.find((observer) => observer.target === root)
  const sidebarObserver = observers.find((observer) => observer.target === currentSidebar)
  assert.equal(shellObserver.options.subtree, undefined)
  assert.equal(sidebarObserver.options.subtree, true)
  assert.equal(observed.size, 3)
  const oldCard = currentSidebar.targets[0]
  const newCard = card()
  currentSidebar.targets[0] = newCard
  sidebarObserver.callback()
  assert.equal(observed.has(oldCard), false)
  assert.equal(observed.has(newCard), true)
  const oldSidebar = currentSidebar
  currentSidebar = sidebar()
  shellObserver.callback()
  assert.equal(observed.has(oldSidebar), false)
  assert.equal(sidebarObserver.target, currentSidebar)
  currentSidebar.collapsed = true
  sidebarObserver.callback()
  assert.equal(observed.size, 0)
  currentSidebar.collapsed = false
  sidebarObserver.callback()
  assert.equal(observed.size, 3)
  assert.equal(layouts, 4)
  dispose()
  assert.equal(observed.size, 0)
  assert.ok(observers.every((observer) => observer.target === null))
})

test('width-only shell animation does not repeatedly realign the file panel', () => {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(
        new URL('../src/renderer/features/workspace/file-panel-layout-observer.ts', import.meta.url),
        'utf8'
      ),
      { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
    ).outputText,
    { exports, require: () => ({}) }
  )
  const workspace = { classList: { contains: () => false } }
  const sidebar = { classList: { contains: (name) => name === 'fs-sidebar' } }
  const target = { classList: { contains: () => false } }
  const heights = new WeakMap()
  const entry = (target, height, width) => ({ target, contentRect: { height, width } })
  const shouldSync = (entries) => exports.shouldSyncFilePanelForResize(entries, workspace, heights)
  assert.equal(shouldSync([entry(workspace, 700, 600), entry(sidebar, 700, 214)]), true)
  for (const width of [180, 120, 44]) {
    assert.equal(shouldSync([entry(workspace, 700, 600 + 214 - width), entry(sidebar, 700, width)]), false)
  }
  assert.equal(shouldSync([entry(workspace, 720, 770)]), true, 'height changes still clamp the panel')
  assert.equal(shouldSync([entry(sidebar, 720, 44)]), true, 'sidebar height changes still realign')
  assert.equal(shouldSync([entry(target, 25, 100)]), true, 'anchor changes still realign')
  assert.equal(shouldSync([entry(target, 25, 120)]), true, 'anchor width changes can move other anchors')
})
