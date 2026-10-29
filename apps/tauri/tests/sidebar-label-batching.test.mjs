import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

test('sidebar label measurements batch layout writes before geometry reads', () => {
  const events = []
  const frames = []
  const cleanups = []
  let index = 0
  const element = () => {
    const id = index++
    return {
      textContent: 'Label',
      style: new Proxy(
        { removeProperty() {} },
        {
          set(target, key, value) {
            events.push(`write:${id}`)
            target[key] = value
            return true
          }
        }
      ),
      parentElement: { querySelector: () => null },
      closest: () => null,
      getBoundingClientRect: () => {
        events.push(`read:${id}`)
        return { top: 0, height: 20 }
      }
    }
  }
  const react = { useRef: () => ({ current: element() }), useLayoutEffect: (fn) => cleanups.push(fn()) }
  const exports = {}
  const metrics = {
    fontBoundingBoxAscent: 10,
    fontBoundingBoxDescent: 4,
    actualBoundingBoxAscent: 9,
    actualBoundingBoxDescent: 3
  }
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../src/renderer/components/common/icon-label/icon-label.tsx', import.meta.url), 'utf8'),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      require: (name) => (name === 'react' ? react : { jsx: () => null }),
      document: {
        createElement: () => ({ getContext: () => ({ measureText: () => metrics }) }),
        documentElement: {},
        fonts: { addEventListener() {}, removeEventListener() {} }
      },
      getComputedStyle: () => ({
        fontStyle: 'normal',
        fontWeight: '400',
        fontSize: '14px',
        fontFamily: 'sans-serif',
        lineHeight: '20px'
      }),
      MutationObserver: class {
        observe() {}
        disconnect() {}
      },
      requestAnimationFrame: (fn) => {
        frames.push(fn)
        return frames.length
      }
    }
  )
  exports.SidebarAlignedLabel({ children: 'First' })
  exports.SidebarAlignedLabel({ children: 'Second' })
  assert.equal(frames.length, 1)
  frames.shift()()
  assert.deepEqual(events, [
    'write:0',
    'write:0',
    'write:0',
    'write:1',
    'write:1',
    'write:1',
    'read:0',
    'read:1',
    'write:0',
    'write:1'
  ])
  cleanups.forEach((cleanup) => cleanup())
})
