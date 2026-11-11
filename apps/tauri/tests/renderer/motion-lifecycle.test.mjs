import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture(file, name, globals = {}) {
  const slots = []
  let index = 0
  let pending = []
  const hook = {
    useRef(value) {
      const slot = index++
      return (slots[slot] ??= { current: value })
    },
    useState(value) {
      const slot = index++
      slots[slot] ??= { value }
      return [
        slots[slot].value,
        (next) => {
          slots[slot].value = next
        }
      ]
    },
    useEffect(effect, deps) {
      const slot = index++
      const previous = slots[slot]
      if (previous && deps.every((value, i) => Object.is(value, previous.deps[i]))) return
      pending.push(() => {
        previous?.cleanup?.()
        slots[slot] = { deps, cleanup: effect() }
      })
    }
  }
  hook.useLayoutEffect = hook.useEffect
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL(`../../src/renderer/features/common/${file}.ts`, import.meta.url), 'utf8'),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      require: (name) => (name === 'react' ? hook : {}),
      getComputedStyle: (node) => ({
        opacity: node.opacity ?? '1',
        transform: node.transform ?? 'none',
        getPropertyValue: (key) => (key.startsWith('--ease') ? 'ease-in-out' : '150ms')
      }),
      ...globals
    }
  )
  return {
    render(args, mount = () => {}) {
      index = 0
      pending = []
      const result = exports[name](...args)
      mount(result)
      pending.forEach((effect) => effect())
      return result
    },
    dispose() {
      slots.forEach((slot) => slot?.cleanup?.())
    }
  }
}

function media() {
  const listeners = new Set()
  return {
    matches: false,
    addEventListener: (_, fn) => listeners.add(fn),
    removeEventListener: (_, fn) => listeners.delete(fn),
    change(value) {
      this.matches = value
      listeners.forEach((fn) => fn())
    }
  }
}

function surface() {
  const animations = []
  const node = {
    style: {},
    opacity: '1',
    transform: 'none',
    animate(frames, options) {
      const animation = {
        frames,
        options,
        cancelled: false,
        cancel() {
          this.cancelled = true
        }
      }
      animations.push(animation)
      return animation
    }
  }
  return { node, animations }
}

test('closing surfaces remain present until exit completes and reopening cancels removal', () => {
  const timers = new Map()
  let timerId = 0
  const window = {
    setTimeout(fn, duration) {
      assert.equal(duration, 150)
      timers.set(++timerId, fn)
      return timerId
    },
    clearTimeout(id) {
      timers.delete(id)
    }
  }
  const runner = fixture('use-exit-presence', 'useExitPresence', { window, document: { documentElement: {} } })
  assert.equal(runner.render([true]), true)
  assert.equal(runner.render([false]), true)
  assert.equal(timers.size, 1)
  assert.equal(runner.render([true]), true)
  assert.equal(timers.size, 0)
  runner.render([false])
  const callback = [...timers.values()][0]
  timers.clear()
  callback()
  assert.equal(runner.render([false]), false)
  runner.dispose()
  assert.equal(timers.size, 0)
})

test('interrupted surface animations resume from the visible frame and honor changing motion preference', () => {
  const reduced = media()
  const runner = fixture('use-surface-motion', 'useSurfaceMotion', { window: { matchMedia: () => reduced } })
  const { node, animations } = surface()
  const ref = { current: node }
  runner.render([ref, true, 'scale(0.96)'])
  assert.equal(animations[0].frames[0].opacity, '0')
  node.opacity = '0.6'
  node.transform = 'matrix(0.98, 0, 0, 0.98, 0, 0)'
  runner.render([ref, false, 'scale(0.96)'])
  assert.equal(animations[0].cancelled, true)
  assert.equal(animations[1].frames[0].opacity, '0.6')
  assert.equal(animations[1].frames[0].transform, node.transform)
  runner.render([ref, true, 'scale(0.96)'])
  assert.equal(animations[1].cancelled, true)
  reduced.change(true)
  const latest = animations.at(-1)
  assert.equal(latest.frames[0].transform, 'none')
  assert.equal(latest.frames[1].transform, 'none')
  runner.dispose()
  assert.equal(latest.cancelled, true)
})

test('selection highlights interrupt at their current position, resize without animation, and stop for reduced motion', () => {
  const reduced = media()
  const observers = []
  const { node, animations } = surface()
  let selectedX = 0
  let hasSelection = true
  let visualX
  const root = {
    dataset: {},
    clientLeft: 0,
    clientTop: 0,
    scrollLeft: 0,
    scrollTop: 0,
    getBoundingClientRect: () => ({ left: 0, top: 0 }),
    querySelector: () =>
      hasSelection ? { getBoundingClientRect: () => ({ left: selectedX, top: 0, width: 100, height: 30 }) } : null
  }
  node.getBoundingClientRect = () => ({
    left: visualX ?? Number.parseFloat(node.style.left || '0'),
    top: 0,
    width: 100,
    height: 30
  })
  const originalAnimate = node.animate
  node.animate = (frames, options) => {
    const animation = originalAnimate(frames, options)
    const cancel = animation.cancel.bind(animation)
    animation.cancel = () => {
      visualX = undefined
      cancel()
    }
    return animation
  }
  const runner = fixture('use-selection-indicator', 'useSelectionIndicator', {
    window: { matchMedia: () => reduced },
    ResizeObserver: class {
      constructor(fn) {
        observers.push(fn)
      }
      observe() {}
      disconnect() {}
    }
  })
  const mount = ({ rootRef, indicatorRef }) => {
    rootRef.current = root
    indicatorRef.current = node
  }
  runner.render(['one', '.selected'], mount)
  assert.equal(animations.length, 0)
  selectedX = 100
  runner.render(['two', '.selected'], mount)
  assert.match(animations[0].frames[0].transform, /translate\(-100px, 0px\)/)
  visualX = 40
  selectedX = 200
  runner.render(['three', '.selected'], mount)
  assert.match(animations[1].frames[0].transform, /translate\(-160px, 0px\)/)
  assert.equal(animations[0].cancelled, true)
  selectedX = 240
  observers.at(-1)()
  assert.equal(animations.length, 2)
  assert.equal(node.style.left, '240px')
  assert.equal(animations[1].cancelled, true)
  reduced.change(true)
  selectedX = 0
  runner.render(['one', '.selected'], mount)
  assert.equal(animations.length, 2)
  hasSelection = false
  runner.render(['missing', '.selected'], mount)
  assert.equal(root.dataset.selectionMotion, undefined)
  hasSelection = true
  runner.render(['one', '.selected'], mount)
  assert.equal(root.dataset.selectionMotion, 'ready')
  runner.dispose()
})
