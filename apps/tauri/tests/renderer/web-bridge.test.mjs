import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(relativePath, modules = {}, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(relativePath, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      URL,
      crypto,
      clearTimeout,
      setTimeout,
      clearInterval,
      setInterval: (fn, ms) => {
        const handle = setInterval(fn, ms)
        if (typeof handle?.unref === 'function') {
          handle.unref()
        }
        return handle
      },
      require: (name) => modules[name] ?? {},
      ...globals
    }
  )
  return exports
}

function createFakeWebSocketClass(sentFrames, sockets) {
  return class FakeWebSocket {
    static OPEN = 1
    readyState = 1
    listeners = new Map()

    constructor(url) {
      this.url = url
      sockets.push(this)
      queueMicrotask(() => this.emit('open', {}))
    }

    addEventListener(type, fn) {
      if (!this.listeners.has(type)) this.listeners.set(type, new Set())
      this.listeners.get(type).add(fn)
    }

    emit(type, event) {
      for (const fn of this.listeners.get(type) ?? []) {
        fn(event)
      }
    }

    send(text) {
      sentFrames.push(JSON.parse(text))
    }

    close() {
      this.readyState = 3
    }
  }
}

test('web bridge bootstraps metadata, handles RPC, dispatches WS events & acks terminal output, and opens web overlays', async () => {
  const fetchCalls = []
  const sentFrames = []
  const sockets = []
  const dispatchedEvents = []
  const downloadLinks = []

  const FakeWebSocket = createFakeWebSocketClass(sentFrames, sockets)
  const windowListeners = new Map()
  const fakeWindow = {
    location: { origin: 'https://fileterm.example:8443', href: 'https://fileterm.example:8443/' },
    addEventListener(type, fn) {
      if (!windowListeners.has(type)) windowListeners.set(type, new Set())
      windowListeners.get(type).add(fn)
    },
    removeEventListener(type, fn) {
      windowListeners.get(type)?.delete(fn)
    },
    dispatchEvent(event) {
      dispatchedEvents.push(event)
    }
  }
  const fakeDocument = {
    body: {
      appendChild(el) {
        downloadLinks.push(el)
      }
    },
    createElement(tag) {
      const el = {
        tag,
        style: {},
        clicked: false,
        click() {
          el.clicked = true
        },
        remove() {}
      }
      return el
    }
  }

  const fakeFetch = async (url, init = {}) => {
    fetchCalls.push({ url, method: init.method ?? 'GET', body: init.body ? JSON.parse(init.body) : null })
    if (url.endsWith('/api/bootstrap')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          platform: 'web',
          arch: 'x64',
          appVersion: '2.3.4',
          appName: 'FileTerm',
          runtimeName: 'Web',
          runtimeVersion: 'web-2.3.4',
          isDesktop: false
        })
      }
    }
    if (url.endsWith('/api/rpc')) {
      const { command, args } = JSON.parse(init.body)
      if (command === 'app_get_ui_preferences') {
        return { ok: true, status: 200, json: async () => ({ data: { theme: 'fileterm-dark' } }) }
      }
      if (command === 'app_fail_test') {
        return { ok: false, status: 400, json: async () => ({ error: 'boom' }) }
      }
      if (command === 'app_download_remote_path') {
        return { ok: true, status: 200, json: async () => ({ data: { tabs: [{ id: args.tabId }] } }) }
      }
      return { ok: true, status: 200, json: async () => ({ data: null }) }
    }
    if (url === '/api/web/prepare-download') {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          token: 'dl-token-1',
          fileName: 'remote.txt',
          snapshot: { tabs: [{ id: 'tab-1' }] }
        })
      }
    }
    throw new Error(`Unexpected fetch URL: ${url}`)
  }

  const globals = {
    window: fakeWindow,
    document: fakeDocument,
    fetch: fakeFetch,
    WebSocket: FakeWebSocket,
    performance,
    Event: class {
      constructor(type) {
        this.type = type
      }
    },
    CustomEvent: class {
      constructor(type, init) {
        this.type = type
        this.detail = init?.detail
      }
    }
  }

  const appEvents = load('../../src/renderer/lib/app-events.ts', {}, globals)
  const terminalOutput = load('../../src/bridge/events/terminal-output.ts', {}, globals)
  const sharedApi = load('../../src/bridge/shared-api.ts', {}, globals)
  const webTransport = load(
    '../../src/bridge/web-transport.ts',
    { './events/terminal-output': terminalOutput },
    globals
  )
  const { createWebApi } = load(
    '../../src/bridge/web-api.ts',
    {
      '../renderer/lib/app-events': appEvents,
      './shared-api': sharedApi,
      './web-transport': webTransport
    },
    globals
  )

  const api = await createWebApi()
  assert.equal(api.isDesktop, false)
  assert.equal(api.platform, 'web')
  assert.equal(api.runtimeName, 'Web')
  assert.equal(sockets.length, 1)
  assert.ok(sockets[0].url.startsWith('wss://fileterm.example:8443/api/ws?clientId='))

  // RPC dispatch
  const prefs = await api.getUiPreferences()
  assert.deepEqual(JSON.parse(JSON.stringify(prefs)), { theme: 'fileterm-dark' })
  assert.equal(fetchCalls[1].body.command, 'app_get_ui_preferences')
  assert.equal(fetchCalls[1].body.windowLabel, 'main')

  // WS broadcast event dispatch
  const receivedEvents = []
  const unsub = api.onTerminalState((payload) => receivedEvents.push(payload))
  sockets[0].emit('message', {
    data: JSON.stringify({ type: 'event', event: 'terminal:state', payload: { tabId: 't1', status: 'connected' } })
  })
  assert.deepEqual(JSON.parse(JSON.stringify(receivedEvents)), [{ tabId: 't1', status: 'connected' }])
  unsub()

  // Terminal subscription + ack flow
  const chunks = []
  api.onTerminalData(async (payload) => {
    chunks.push(payload)
  })
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.ok(sentFrames.some((frame) => frame.type === 'subscribe_terminal'))

  sockets[0].emit('message', {
    data: JSON.stringify({
      type: 'event',
      event: 'web:terminal-subscribed',
      payload: { channelId: 77 }
    })
  })
  sockets[0].emit('message', {
    data: JSON.stringify({
      type: 'terminal_data',
      channelId: 77,
      payload: { deliveryId: 3, tabId: 't1', chunk: 'ls\r\n' }
    })
  })
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.equal(chunks.length, 1)
  assert.equal(chunks[0].chunk, 'ls\r\n')
  assert.deepEqual(JSON.parse(JSON.stringify(sentFrames.find((frame) => frame.type === 'ack_terminal'))), {
    type: 'ack_terminal',
    channelId: 77,
    deliveryId: 3
  })

  // Modal overlay events replace native child windows
  await api.openConnectionManagerWindow()
  await api.openConnectionFormWindow('edit', 'prof-1')
  await api.openFileEditorWindow({ source: 'remote', path: '/etc/hosts', name: 'hosts', tabId: 't1' })
  assert.deepEqual(JSON.parse(JSON.stringify(dispatchedEvents.map((e) => ({ type: e.type, detail: e.detail })))), [
    { type: appEvents.APP_EVENT.webOpenWindow, detail: { kind: 'connection-manager' } },
    {
      type: appEvents.APP_EVENT.webOpenWindow,
      detail: { kind: 'connection-form', mode: 'edit', profileId: 'prof-1' }
    },
    {
      type: appEvents.APP_EVENT.webOpenWindow,
      detail: { kind: 'file-editor', source: 'remote', path: '/etc/hosts', name: 'hosts', tabId: 't1' }
    }
  ])

  // Empty localDirectory downloads via browser /api/web/prepare-download, non-empty uses server RPC
  await api.downloadRemotePath('tab-1', '/etc/hosts', 'file', '')
  assert.equal(downloadLinks.length, 1)
  assert.equal(downloadLinks[0].href, '/api/web/downloads/dl-token-1')
  assert.equal(downloadLinks[0].download, 'remote.txt')
  assert.equal(downloadLinks[0].clicked, true)

  await api.downloadRemotePath('tab-1', '/etc/hosts', 'file', '/tmp')
  assert.equal(fetchCalls.at(-1).url, 'https://fileterm.example:8443/api/rpc')
  assert.equal(fetchCalls.at(-1).body.command, 'app_download_remote_path')

  for (const fn of windowListeners.get('beforeunload') ?? []) {
    fn()
  }
  assert.equal(sockets[0].readyState, 3)
})
