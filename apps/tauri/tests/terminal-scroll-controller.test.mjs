import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../src/renderer/components/terminal-scroll-controller.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports, queueMicrotask }
)

function fixture() {
  const events = new Map()
  const subscribe = (name) => (listener) => {
    events.set(name, listener)
    return { dispose: () => events.delete(name) }
  }
  const terminal = {
    rows: 10,
    buffer: { active: { length: 100, viewportY: 90 }, onBufferChange: subscribe('buffer') },
    onScroll: subscribe('scroll'),
    onResize: subscribe('resize'),
    onRender: subscribe('render'),
    onWriteParsed: subscribe('parsed'),
    scrollToLine: (line) => {
      terminal.line = line
    },
    scrollLines: (lines) => {
      terminal.lines = lines
    }
  }
  const ref = { current: terminal }
  const element = { clientHeight: 200 }
  const controller = exports.createTerminalScrollController(ref, element)
  return { events, terminal, ref, element, controller }
}

test('continuous output coalesces scroll and parse notifications without animation frames', async () => {
  const f = fixture()
  let notifications = 0
  const dispose = f.controller.subscribe(() => notifications++)
  f.terminal.buffer.active.length = 110
  f.terminal.buffer.active.viewportY = 100
  for (let index = 0; index < 1000; index++) f.events.get('scroll')()
  f.events.get('parsed')()
  await Promise.resolve()
  assert.equal(notifications, 1)
  assert.equal(f.events.has('render'), false, 'cursor repaint must not reveal or update the scrollbar')
  f.terminal.rows = 12
  f.events.get('resize')()
  f.events.get('buffer')()
  await Promise.resolve()
  assert.equal(notifications, 2)
  dispose()
  assert.equal(f.events.size, 0)
})

test('redraws in separate parser tasks do not notify when scroll metrics stay unchanged', async () => {
  const f = fixture()
  let notifications = 0
  const dispose = f.controller.subscribe(() => notifications++)
  for (let index = 0; index < 100; index++) {
    f.events.get('parsed')()
    await Promise.resolve()
  }
  assert.equal(notifications, 0)
  // Growing history must still notify even when the user has scrolled away from the bottom.
  f.terminal.buffer.active.length++
  f.events.get('parsed')()
  await Promise.resolve()
  assert.equal(notifications, 1)
  f.terminal.buffer.active.viewportY--
  f.events.get('scroll')()
  await Promise.resolve()
  assert.equal(notifications, 2)
  f.terminal.buffer.active = { length: 10, viewportY: 0 }
  f.events.get('buffer')()
  await Promise.resolve()
  assert.equal(notifications, 3)
  for (let index = 0; index < 100; index++) {
    f.events.get('parsed')()
    await Promise.resolve()
  }
  assert.equal(notifications, 3)
  dispose()
})

test('unsubscribe and terminal replacement discard pending notifications', async () => {
  for (const stop of ['unsubscribe', 'replace']) {
    const f = fixture()
    let notifications = 0
    const dispose = f.controller.subscribe(() => notifications++)
    f.terminal.buffer.active.viewportY--
    f.events.get('scroll')()
    if (stop === 'unsubscribe') dispose()
    else f.ref.current = null
    await Promise.resolve()
    assert.equal(notifications, 0)
    dispose()
  }
})

test('metrics and input keep pixel-to-row mapping for scrollback, alternate screen and hidden terminals', () => {
  const f = fixture()
  assert.equal(
    JSON.stringify(f.controller.getMetrics()),
    JSON.stringify({ clientHeight: 200, scrollHeight: 2000, scrollTop: 1800 })
  )
  f.controller.scrollTo(400)
  f.controller.scrollBy(-40)
  assert.equal(f.terminal.line, 20)
  assert.equal(f.terminal.lines, -2)
  f.terminal.buffer.active = { length: 10, viewportY: 0 }
  assert.equal(f.controller.getMetrics().scrollHeight, 200)
  f.element.clientHeight = 0
  assert.equal(f.controller.getMetrics(), null)
  f.ref.current = null
  f.controller.scrollBy(100)
  assert.equal(f.controller.getMetrics(), null)
})

test('observed viewport height avoids a synchronous element measurement', () => {
  const f = fixture()
  Object.defineProperty(f.element, 'clientHeight', {
    get() {
      throw new Error('unexpected layout read')
    }
  })
  const metrics = f.controller.getMetrics(300)
  assert.equal(metrics.clientHeight, 300)
  assert.equal(metrics.scrollHeight, 3000)
  assert.equal(metrics.scrollTop, 2700)
  assert.equal(f.controller.getMetrics(0), null)
})
