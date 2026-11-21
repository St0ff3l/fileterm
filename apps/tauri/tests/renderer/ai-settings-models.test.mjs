import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function loadTs(relativePath, dependencies = {}, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(relativePath, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        if (!(name in dependencies)) throw Error(`Unexpected dependency: ${name}`)
        return dependencies[name]
      },
      ...globals
    }
  )
  return exports
}

const { reorderAiProviderModels } = loadTs(
  '../../src/renderer/features/settings/settings-modal/controller/model-order.ts'
)
const { getAiReasoningProfile } = loadTs('../../src/renderer/features/ai/ai-reasoning.ts', {
  '../settings/settings-modal/constants': { isAiManualCapabilityProvider: () => false }
})

test('provider model order moves chips before or after their target and preserves no-op lists', () => {
  const models = ['deepseek-v4-pro', 'deepseek-flash', 'other']
  assert.deepEqual(
    [...reorderAiProviderModels(models, 'other', 'deepseek-v4-pro', 'before')],
    ['other', 'deepseek-v4-pro', 'deepseek-flash']
  )
  assert.deepEqual(
    [...reorderAiProviderModels(models, 'deepseek-v4-pro', 'other', 'after')],
    ['deepseek-flash', 'other', 'deepseek-v4-pro']
  )
  assert.equal(reorderAiProviderModels(models, 'missing', 'other', 'after'), models)
})

test('the current DeepSeek Flash model exposes its reasoning effort selector', () => {
  const profile = getAiReasoningProfile(
    {
      kind: 'openai-compatible-chat',
      baseUrl: 'https://api.deepseek.com',
      name: 'DeepSeek',
      modelCapabilities: {}
    },
    'deepseek-flash'
  )
  assert.equal(profile.control, 'effort')
  assert.deepEqual([...profile.efforts], ['none', 'low', 'high', 'max'])
})

test('pointer sorting does not capture clicks and captures only after the drag threshold', () => {
  const listeners = new Map()
  let pointerCaptureCalls = 0
  class MockElement {
    constructor(attributes = []) {
      this.attributes = new Set(attributes)
      this.classList = { add() {}, remove() {} }
      this.style = {}
      this.captured = false
    }
    closest() {
      return this
    }
    hasAttribute(name) {
      return this.attributes.has(name)
    }
    setAttribute() {}
    getBoundingClientRect() {
      return { left: 0, top: 0, width: 100, height: 26 }
    }
    cloneNode() {
      return new MockElement()
    }
    setPointerCapture() {
      this.captured = true
      pointerCaptureCalls += 1
    }
    hasPointerCapture() {
      return this.captured
    }
    releasePointerCapture() {
      this.captured = false
    }
    remove() {}
  }
  const window = {
    addEventListener: (name, listener) => listeners.set(name, listener),
    removeEventListener() {},
    getComputedStyle: () => ({
      display: 'inline-flex',
      gridTemplateColumns: 'none',
      columnGap: '0px',
      padding: '0px',
      border: '1px solid',
      borderRadius: '6px',
      background: '#2a2a2a',
      color: '#eee',
      font: '11px monospace',
      boxSizing: 'border-box'
    })
  }
  const document = {
    body: { appendChild() {} },
    documentElement: { classList: { add() {}, remove() {} } },
    elementFromPoint: () => null
  }
  let dragStarts = 0
  const { usePointerSortFallback } = loadTs(
    '../../src/renderer/hooks/use-pointer-sort-fallback.ts',
    { react: { useEffect: (effect) => effect(), useRef: (current) => ({ current }) } },
    { window, document, Element: MockElement }
  )
  const handlePointerDown = usePointerSortFallback({
    onStart: () => dragStarts++,
    onTarget() {},
    onDrop() {},
    onCancel() {}
  })
  const sourceElement = new MockElement()
  const pointerDown = (clientX, pointerId) => ({
    button: 0,
    isPrimary: true,
    target: new MockElement(['data-fileterm-sort-handle']),
    currentTarget: sourceElement,
    clientX,
    clientY: 10,
    pointerId
  })

  handlePointerDown(pointerDown(10, 1), 'model')
  assert.equal(pointerCaptureCalls, 0)
  listeners.get('pointerup')({ clientX: 10, clientY: 10, pointerId: 1 })
  assert.equal(dragStarts, 0)

  handlePointerDown(pointerDown(10, 2), 'model')
  listeners.get('pointermove')({
    clientX: 16,
    clientY: 10,
    pointerId: 2,
    preventDefault() {}
  })
  assert.equal(pointerCaptureCalls, 1)
  assert.equal(dragStarts, 1)
})
