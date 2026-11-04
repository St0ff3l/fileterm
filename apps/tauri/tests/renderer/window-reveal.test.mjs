import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture() {
  const frames = new Map()
  const timers = new Map()
  let nextId = 0
  let reveals = 0
  let cleanup
  const exports = {}
  const source = readFileSync(new URL('../../src/renderer/hooks/use-window-reveal.ts', import.meta.url), 'utf8')
  vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, {
    exports,
    console,
    window: {
      setTimeout(callback) {
        timers.set(++nextId, callback)
        return nextId
      },
      clearTimeout: (id) => timers.delete(id)
    },
    requestAnimationFrame(callback) {
      frames.set(++nextId, callback)
      return nextId
    },
    cancelAnimationFrame: (id) => frames.delete(id),
    require(name) {
      if (name === 'react') return { useEffect: (effect) => (cleanup = effect()) }
      if (name === '../../bridge/tauri-api') {
        return {
          showCurrentWindow: async () => {
            reveals++
          }
        }
      }
      throw new Error(`Unexpected import: ${name}`)
    }
  })
  const flush = (queue) => {
    const pending = [...queue.values()]
    queue.clear()
    pending.forEach((callback) => callback())
  }
  return {
    render: exports.useWindowReveal,
    cleanup: () => cleanup?.(),
    frame: () => flush(frames),
    fallback: () => flush(timers),
    get reveals() {
      return reveals
    }
  }
}

test('keeps an unready form hidden and reveals only after two ready frames', () => {
  const window = fixture()
  window.render(false)
  window.frame()
  window.fallback()
  assert.equal(window.reveals, 0)
  window.render(true)
  window.frame()
  assert.equal(window.reveals, 0)
  window.frame()
  assert.equal(window.reveals, 1)
  window.fallback()
  assert.equal(window.reveals, 1)
})

test('reveals a ready form if hidden WebViews throttle animation frames', () => {
  const window = fixture()
  window.render(true)
  window.fallback()
  assert.equal(window.reveals, 1)
  window.frame()
  window.frame()
  assert.equal(window.reveals, 1)
})

test('cancels pending reveals across unmount and StrictMode remount', () => {
  const window = fixture()
  window.render(true)
  window.frame()
  window.cleanup()
  window.frame()
  window.fallback()
  assert.equal(window.reveals, 0)
  window.render(true)
  window.frame()
  window.frame()
  assert.equal(window.reveals, 1)
})
