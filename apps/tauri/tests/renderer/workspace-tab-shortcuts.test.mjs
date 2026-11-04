import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(path, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports, ...globals }
  )
  return exports
}

const tabs = [
  { key: 'home:start', kind: 'local', id: 'start', tabKind: 'home' },
  { key: 'session:nas', kind: 'session', tab: { id: 'nas' } },
  { key: 'home:files', kind: 'local', id: 'files', tabKind: 'transfers' }
]

function fixture(platform, entries = tabs, activeKey = entries[0]?.key) {
  const listeners = []
  const activated = []
  const state = { modal: false, remoteInputs: 0 }
  let cleanup
  const window = {
    fileterm: { platform },
    addEventListener(type, handler, capture) {
      listeners.push({ type, handler, capture })
    },
    removeEventListener(type, handler, capture) {
      const index = listeners.findIndex(
        (entry) => entry.type === type && entry.handler === handler && entry.capture === capture
      )
      if (index >= 0) listeners.splice(index, 1)
    }
  }
  const { useWorkspaceTabShortcuts } = load('../../src/renderer/features/layout/use-workspace-tab-shortcuts.ts', {
    window,
    document: { querySelector: () => (state.modal ? {} : null) },
    require: () => ({
      useEffect: (effect) => {
        cleanup = effect()
      }
    })
  })
  useWorkspaceTabShortcuts({
    tabs: entries,
    activeKey,
    onActivateHome: (id) => activated.push(`home:${id}`),
    onActivateSession: (id) => activated.push(`session:${id}`)
  })
  const fire = (key, options = {}) => {
    const event = {
      key,
      ctrlKey: false,
      metaKey: false,
      altKey: false,
      shiftKey: false,
      isComposing: false,
      defaultPrevented: false,
      repeat: false,
      type: 'keydown',
      propagationStopped: false,
      preventDefault() {
        this.defaultPrevented = true
      },
      stopPropagation() {
        this.propagationStopped = true
      },
      ...options
    }
    for (const { type, handler, capture } of listeners) {
      if (type === event.type && capture) handler(event)
    }
    if (!event.propagationStopped) state.remoteInputs++
    return event
  }
  return { fire, activated, state, window, cleanup: () => cleanup?.(), listeners }
}

test('Ctrl+Tab cycles in displayed order on every platform, wraps and never reaches xterm', () => {
  for (const platform of ['darwin', 'win32', 'linux']) {
    const { fire, activated, state } = fixture(platform)
    for (let i = 0; i < 3; i++) {
      assert.equal(fire('Tab', { ctrlKey: true, repeat: i > 0 }).defaultPrevented, true)
    }
    fire('Tab', { ctrlKey: true, shiftKey: true })
    assert.deepEqual(activated, ['session:nas', 'home:files', 'home:start', 'home:files'])
    assert.equal(state.remoteInputs, 0)
  }
})

test('Chrome platform aliases only match the intended modifiers', () => {
  for (const platform of ['win32', 'linux']) {
    const { fire, activated } = fixture(platform)
    fire('PageUp', { ctrlKey: true })
    fire('PageDown', { ctrlKey: true })
    assert.deepEqual(activated, ['home:files', 'home:start'])
    assert.equal(fire('ArrowRight', { metaKey: true, altKey: true }).defaultPrevented, false)
  }
  const { fire, activated } = fixture('darwin')
  fire('ArrowLeft', { metaKey: true, altKey: true })
  fire('ArrowRight', { metaKey: true, altKey: true })
  assert.deepEqual(activated, ['home:files', 'home:start'])
  assert.equal(fire('PageDown', { ctrlKey: true }).defaultPrevented, false)
})

test('number selection uses platform modifiers and only the supplied visible tab group', () => {
  for (const platform of ['darwin', 'win32', 'linux']) {
    const { fire, activated } = fixture(platform, [tabs[1], tabs[0], tabs[2]], tabs[1].key)
    const modifier = platform === 'darwin' ? { metaKey: true } : { ctrlKey: true }
    fire('2', modifier)
    fire('9', modifier)
    fire('1', modifier)
    assert.equal(fire('8', modifier).defaultPrevented, true)
    assert.deepEqual(activated, ['home:start', 'home:files', 'session:nas'])
    assert.equal(fire('0', modifier).defaultPrevented, false)
    assert.equal(fire('2', platform === 'darwin' ? { ctrlKey: true } : { metaKey: true }).defaultPrevented, false)
  }
})

test('terminal, OS and extended modifier combinations retain their existing owners', () => {
  for (const platform of ['darwin', 'win32', 'linux']) {
    const { fire, activated } = fixture(platform)
    for (const [key, modifiers] of [
      ['Tab', {}],
      ['Tab', { altKey: true }],
      ['Tab', { metaKey: true }],
      ['Tab', { ctrlKey: true, altKey: true }],
      ['Tab', { ctrlKey: true, metaKey: true }],
      ['w', { ctrlKey: true }],
      ['w', { ctrlKey: true, shiftKey: true }],
      ['d', { metaKey: true }],
      ['+', { ctrlKey: true, shiftKey: true }],
      ['PageDown', { ctrlKey: true, shiftKey: true }],
      ['1', { ctrlKey: true, altKey: true }]
    ]) {
      assert.equal(fire(key, modifiers).defaultPrevented, false)
    }
    assert.deepEqual(activated, [])
  }
})

test('modal and lock dialogs, composition and already handled events block tab switching', () => {
  const { fire, activated, state } = fixture('darwin')
  state.modal = true
  fire('Tab', { ctrlKey: true })
  state.modal = false
  fire('Tab', { ctrlKey: true, isComposing: true })
  fire('Tab', { ctrlKey: true, defaultPrevented: true })
  assert.deepEqual(activated, [])
})

test('single and empty tab lists avoid redundant activation, and unmount removes the handler', () => {
  const single = fixture('linux', [tabs[0]])
  assert.equal(single.fire('Tab', { ctrlKey: true }).defaultPrevented, true)
  assert.deepEqual(single.activated, [])
  single.cleanup()
  assert.equal(single.listeners.length, 0)
  assert.equal(single.fire('Tab', { ctrlKey: true }).defaultPrevented, false)
  const empty = fixture('linux', [])
  assert.equal(empty.listeners.length, 0)
})

test('tab chords cannot be mistaken for double modifier taps, even when tracker registers later', () => {
  let now = 1000
  const { createTerminalModifierTapTracker } = load('../../src/renderer/features/terminal/terminal-modifier-tap.ts', {
    Date: { now: () => now }
  })
  for (const modifier of ['Control', 'Meta']) {
    const { fire, window } = fixture('darwin')
    const tracker = createTerminalModifierTapTracker()
    let doubleTaps = 0
    window.addEventListener('keydown', tracker.keyDown, true)
    window.addEventListener(
      'keyup',
      (event) => {
        if (tracker.keyUp(event)) doubleTaps++
      },
      true
    )
    const modifiers = modifier === 'Control' ? { ctrlKey: true } : { metaKey: true }
    const tap = (chord) => {
      fire(modifier, modifiers)
      if (chord) fire(modifier === 'Control' ? 'Tab' : '2', modifiers)
      fire(modifier, { type: 'keyup' })
      now += 100
    }
    tap(true)
    tap(true)
    assert.equal(doubleTaps, 0)
    tap(false)
    tap(true)
    assert.equal(doubleTaps, 0)
    tap(false)
    tap(false)
    assert.equal(doubleTaps, 1)
    tap(false)
    tracker.reset()
    tap(false)
    assert.equal(doubleTaps, 1)
  }
})
