import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture({ supportsLongTasks = false, bridgeReady = true, longTaskObserveFails = false } = {}) {
  let now = 0
  let callback
  let reset
  let cleared = false
  let longTaskCallback
  let longFrameCallback
  let observerDisconnected = false
  const calls = []
  let work = {}
  let output = null
  let interaction = { supported: false, samples: 0 }

  const document = {
    hidden: false,
    hasFocus: () => true,
    querySelectorAll: (selector) => (selector === 'canvas' ? [{ width: 10, height: 20 }] : Array(10)),
    getElementsByTagName: () => Array(8000),
    addEventListener: (_, listener) => {
      reset = listener
    },
    removeEventListener: (_, listener) => {
      assert.equal(listener, reset)
      reset = undefined
    }
  }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../../src/renderer/lib/renderer-performance-log.ts', import.meta.url), 'utf8'),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      document,
      performance: { now: () => now, timeOrigin: 100000, memory: { usedJSHeapSize: 1234, totalJSHeapSize: 4567 } },
      PerformanceObserver: supportsLongTasks
        ? class {
            static supportedEntryTypes = ['longtask']
            constructor(callback) {
              longTaskCallback = callback
            }
            observe(options) {
              assert.equal(options.type, 'longtask')
              if (longTaskObserveFails) throw new Error('unsupported WebView observer')
            }
            disconnect() {
              observerDisconnected = true
            }
          }
        : undefined,
      window: {
        get fileterm() {
          return bridgeReady ? { appVersion: '2.2.31', platform: 'win32' } : undefined
        },
        __TAURI_INTERNALS__: { callbacks: new Map([[1, () => {}]]) },
        setInterval: (listener, interval) => {
          assert.equal(interval, 1000)
          callback = listener
          return 7
        },
        clearInterval: (timer) => {
          assert.equal(timer, 7)
          cleared = true
        }
      },
      require: (name) =>
        name.includes('renderer-work-log')
          ? {
              getRendererWorkSnapshot: () => ({ phases: { ...work }, recentSlowWork: [] }),
              resetRendererWork: () => {
                work = {}
              }
            }
          : name.includes('renderer-frame-log')
            ? {
                observeRendererFrames: (listener) => {
                  longFrameCallback = listener
                  return {
                    snapshot: () => ({ supported: false }),
                    reset: () => {},
                    dispose: () => {}
                  }
                }
              }
            : name.includes('terminal-diagnostics')
              ? { getTerminalResourceSnapshot: () => ({ terminals: 10 }) }
              : name.includes('terminal-output')
                ? {
                    getTerminalOutputProgress: () => output
                  }
                : name.includes('renderer-interaction-performance')
                  ? {
                      observeInteractionPerformance: () => ({
                        snapshot: () => interaction,
                        reset: () => {
                          interaction = { supported: false, samples: 0 }
                        },
                        dispose: () => {}
                      })
                    }
                  : {
                      DIAGNOSTIC_SCOPES: { performance: 'renderer:performance' },
                      writeDiagnosticLog: (...args) => calls.push(args)
                    }
    }
  )
  const dispose = exports.installRendererPerformanceLogging()
  const startup = calls.length ? JSON.parse(calls.shift()[2]) : null
  return {
    startup,
    makeBridgeReady: () => {
      bridgeReady = true
    },
    calls,
    emitLongFrame: () => longFrameCallback(),
    setWork: (value) => {
      work = value
    },
    setOutput: (value) => {
      output = value
    },
    setInteraction: (value) => {
      interaction = value
    },
    document,
    dispose,
    get cleared() {
      return cleared
    },
    get observerDisconnected() {
      return observerDisconnected
    },
    longTasks: (...durations) =>
      longTaskCallback({ getEntries: () => durations.map((duration) => ({ startTime: now, duration })) }),
    tick: (elapsed) => {
      now += elapsed
      callback()
    },
    visibility: () => reset()
  }
}

test('startup identifies the installed diagnostics schema without waiting for a stall', () => {
  const f = fixture()
  assert.equal(f.startup.event, 'renderer-diagnostics-ready')
  assert.equal(f.startup.schemaVersion, 3)
  assert.equal(f.startup.platform, 'win32')
  f.dispose()
})

test('recovered foreground stalls log WARN without an exception and are rate limited', () => {
  const f = fixture()
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.tick(5000)
  assert.equal(f.calls[0][0], 'WARN')
  assert.equal(f.calls[0][1], 'renderer:performance')
  assert.equal(JSON.parse(f.calls[0][2]).elapsedMs, 5000)
  assert.equal(JSON.parse(f.calls[0][2]).lastHeartbeatAtUnixMs, 101000)
  assert.equal(JSON.parse(f.calls[0][2]).expectedNextHeartbeatAtUnixMs, 102000)
  assert.equal(JSON.parse(f.calls[0][2]).resumedAtUnixMs, 106000)
  f.tick(5000)
  assert.equal(f.calls.length, 1)
  f.tick(30000)
  assert.equal(f.calls.length, 2)
  f.dispose()
  assert.equal(f.cleared, true)
})

test('two-second heartbeat gaps are logged with the expected wake and recovery times', () => {
  const f = fixture()
  f.tick(2000)
  const gap = JSON.parse(f.calls[0][2])
  assert.equal(gap.event, 'event-loop-gap')
  assert.equal(gap.delayedByMs, 1000)
  assert.equal(gap.lastHeartbeatAtUnixMs, 100000)
  assert.equal(gap.expectedNextHeartbeatAtUnixMs, 101000)
  assert.equal(gap.resumedAtUnixMs, 102000)
  f.dispose()
})

test('long frames report immediately with context and at most once per thirty seconds', () => {
  const f = fixture()
  f.setWork({ 'xterm-render': { calls: 1, maxElapsedMs: 2000 } })
  f.emitLongFrame()
  f.emitLongFrame()
  assert.equal(f.calls.length, 1)
  const first = JSON.parse(f.calls[0][2])
  assert.equal(first.event, 'renderer-long-frame')
  assert.equal(first.work.phases['xterm-render'].maxElapsedMs, 2000)
  assert.equal(first.terminalResources.terminals, 10)
  for (let index = 0; index < 30; index++) f.tick(1000)
  f.emitLongFrame()
  assert.equal(f.calls.length, 2)
  f.dispose()
})

test('hidden timer throttling and the transition back to foreground are ignored', () => {
  const f = fixture()
  f.document.hidden = true
  f.tick(60000)
  f.document.hidden = false
  f.visibility()
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.tick(4000)
  assert.equal(f.calls.length, 1)
  f.dispose()
})

test('minute health reports include bounded counters without terminal contents', () => {
  const f = fixture()
  for (let index = 0; index < 60; index++) f.tick(1000)
  assert.equal(f.calls.length, 1)
  const health = JSON.parse(f.calls[0][2])
  assert.equal(health.event, 'renderer-health')
  assert.equal(health.uptimeMs, 60000)
  assert.equal(health.usedJSHeapBytes, 1234)
  assert.equal(health.totalJSHeapBytes, 4567)
  assert.equal(health.ipcCallbackCount, 1)
  assert.equal(health.terminalCount, 10)
  assert.equal(health.domNodeCount, 8000)
  assert.equal(health.maxEventLoopGapMs, 1000)
  assert.equal(health.longTaskCount, undefined)
  f.dispose()
})

test('long task summaries reset each minute and across visibility, then disconnect', () => {
  const f = fixture({ supportsLongTasks: true })
  f.longTasks(50, 123)
  for (let index = 0; index < 60; index++) f.tick(1000)
  const health = JSON.parse(f.calls[0][2])
  assert.equal(health.longTaskCount, 2)
  assert.equal(health.longTaskTotalMs, 173)
  assert.equal(health.maxLongTaskMs, 123)
  f.document.hidden = true
  f.longTasks(999)
  f.document.hidden = false
  f.visibility()
  for (let index = 0; index < 60; index++) f.tick(1000)
  assert.equal(JSON.parse(f.calls[1][2]).longTaskCount, 0)
  f.dispose()
  assert.equal(f.observerDisconnected, true)
})

test('stall warnings retain the four worst task timestamps and reset with the health window', () => {
  const f = fixture({ supportsLongTasks: true })
  f.tick(1000)
  f.longTasks(60, 1200, 2400, 300, 900)
  f.tick(3000)
  const gap = JSON.parse(f.calls[0][2])
  assert.equal(gap.longTasks.supported, true)
  assert.equal(gap.longTasks.count, 5)
  assert.equal(gap.longTasks.windowStartedAtMs, 0)
  assert.deepEqual(
    gap.longTasks.slowest.map((task) => task.durationMs),
    [2400, 1200, 900, 300]
  )
  assert.equal(gap.longTasks.slowest[0].startedAtMs, 1000)
  assert.equal(gap.visible, true)
  assert.equal(gap.focused, true)
  for (let index = 0; index < 56; index++) f.tick(1000)
  f.tick(2000)
  const nextGap = JSON.parse(f.calls.at(-1)[2])
  assert.equal(nextGap.longTasks.count, 0)
  assert.equal(nextGap.longTasks.windowStartedAtMs, 60000)
  assert.deepEqual(nextGap.longTasks.slowest, [])
  f.dispose()
})

test('an advertised but rejecting long-task observer does not stop diagnostics installation', () => {
  const f = fixture({ supportsLongTasks: true, longTaskObserveFails: true })
  f.tick(2000)
  const gap = JSON.parse(f.calls[0][2])
  assert.equal(gap.longTasks.supported, false)
  assert.equal(gap.longTasks.count, undefined)
  f.dispose()
})

test('stalled output and slow interactions report bounded summaries on the existing heartbeat', () => {
  const f = fixture()
  f.setOutput({ pendingBatches: 4, busyForMs: 6000, lastReceivedDeliveryId: 8 })
  f.setInteraction({ supported: true, samples: 2, maxInputDelayMs: 1200, maxDurationMs: 1504 })
  f.tick(1000)
  const report = JSON.parse(f.calls[0][2])
  assert.equal(report.event, 'renderer-progress-slow')
  assert.equal(report.terminalOutput.lastReceivedDeliveryId, 8)
  assert.equal(report.interaction.maxInputDelayMs, 1200)
  for (let i = 0; i < 20; i++) f.tick(1000)
  assert.equal(f.calls.length, 1)
  f.setOutput({ pendingBatches: 4, busyForMs: 30000, sinceLastParsedMs: 1 })
  f.setInteraction({ supported: false, samples: 0 })
  for (let i = 0; i < 39; i++) f.tick(1000)
  assert.equal(f.calls.length, 2)
  assert.equal(JSON.parse(f.calls[1][2]).event, 'renderer-health')
  f.dispose()
})

test('input dispatch delay alone triggers a slow interaction warning', () => {
  const f = fixture()
  f.setInteraction({ supported: true, samples: 1, maxInputDelayMs: 300, maxDurationMs: 400 })
  f.tick(1000)
  const report = JSON.parse(f.calls[0][2])
  assert.equal(report.event, 'renderer-progress-slow')
  assert.equal(report.interaction.maxInputDelayMs, 300)
  f.dispose()
})

test('new output after a long idle period is not mistaken for a stalled parse', () => {
  const f = fixture()
  f.setOutput({ pendingBatches: 1, busyForMs: 100, sinceLastParsedMs: 60000 })
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.dispose()
})

test('minute health and a coincident stall share evidence before summaries reset', () => {
  const f = fixture()
  f.setWork({ 'xterm-parse': { calls: 1, totalElapsedMs: 400, maxElapsedMs: 400 } })
  f.tick(60000)
  const health = JSON.parse(f.calls[0][2])
  const gap = JSON.parse(f.calls[1][2])
  assert.equal(health.event, 'renderer-health')
  assert.equal(gap.event, 'event-loop-gap')
  assert.equal(gap.work.phases['xterm-parse'].maxElapsedMs, 400)
  assert.deepEqual(health.work, gap.work)
  assert.equal(health.canvasBackingBytesEstimate, 10 * 20 * 4)
  assert.equal(health.terminalResources.terminals, 10)
  f.tick(60000)
  assert.deepEqual(JSON.parse(f.calls[2][2]).work.phases, {})
  f.dispose()
})

test('startup marker waits for the asynchronously initialized bridge and emits only once', () => {
  const f = fixture({ bridgeReady: false })
  assert.equal(f.startup, null)
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.makeBridgeReady()
  f.tick(1000)
  assert.equal(JSON.parse(f.calls[0][2]).event, 'renderer-diagnostics-ready')
  f.tick(1000)
  assert.equal(f.calls.length, 1)
  f.dispose()
})
