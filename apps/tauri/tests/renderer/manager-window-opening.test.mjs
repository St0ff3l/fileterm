import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const proxy = { id: 'proxy', name: 'Proxy', type: 'socks5', host: 'localhost', port: 1080 }
const event = { stopPropagation() {}, preventDefault() {} }

function loadComponent(path, api, library = {}) {
  const exports = {}
  const jsx = (type, props) => ({ type, props })
  const source = readFileSync(new URL(`../../src/renderer/features/${path}.tsx`, import.meta.url), 'utf8')
  vm.runInNewContext(
    ts.transpileModule(source, {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    {
      exports,
      console,
      window: { fileterm: api },
      require(name) {
        if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx }
        if (name === 'react') {
          return {
            useState: (initial) => [initial, () => {}],
            useRef: (initial) => ({ current: initial }),
            useMemo: (factory) => factory(),
            useEffect() {}
          }
        }
        if (name.endsWith('/use-proxy-library')) return { useProxyLibrary: () => ({ proxies: [proxy], ...library }) }
        if (name.endsWith('/use-tunnel-library')) return { useTunnelLibrary: () => ({ tunnels: [] }) }
        if (name.endsWith('/use-pointer-sort-fallback')) return { usePointerSortFallback: () => () => {} }
        if (name.endsWith('/manager-interactions')) return { targetsNestedManagerControl: (value) => value.nested }
        if (name.endsWith('/manager-drag')) return { managerDropClass: () => '' }
        if (name.endsWith('/i18n')) return { t: {} }
        if (name.endsWith('/use-window-reveal')) return { useWindowReveal: library.onReveal }
        return {}
      }
    }
  )
  return exports
}

function find(element, predicate) {
  if (!element || typeof element !== 'object') return null
  if (Array.isArray(element)) return element.map((child) => find(child, predicate)).find(Boolean)
  return predicate(element) ? element : find(element.props?.children, predicate)
}

test('a proxy row double click opens once and ignores another request while opening', async () => {
  let calls = 0
  let finish
  const pending = new Promise((resolve) => (finish = resolve))
  const { ProxyManagerPage } = loadComponent('proxies/proxy-manager-page', {
    openProxyFormWindow: () => {
      calls++
      return pending
    }
  })
  const element = find(ProxyManagerPage(), (node) => node.type?.name === 'ManagerRow')
  const row = element.type(element.props)
  row.props.onClick?.(event)
  row.props.onClick?.(event)
  row.props.onDoubleClick(event)
  assert.equal(calls, 1)
  row.props.onDoubleClick(event)
  assert.equal(calls, 1)
  finish()
  await pending
  await new Promise(setImmediate)
  row.props.onDoubleClick(event)
  assert.equal(calls, 2)
  row.props.onDoubleClick({ ...event, nested: true })
  assert.equal(calls, 2)
})

test('command editor requests are guarded until the native open finishes', async () => {
  let calls = 0
  let finish
  const pending = new Promise((resolve) => (finish = resolve))
  const { CommandManagerModal } = loadComponent('commands/command-manager-modal', {
    openCommandFormWindow: () => {
      calls++
      return pending
    }
  })
  const tree = CommandManagerModal({
    commandFolders: [],
    commandTemplates: [{ id: 'command', type: 'command', name: 'Command', command: 'pwd' }],
    inline: true
  })
  const row = find(tree, (node) => node.props?.['data-fileterm-sort-id'] === 'command')
  row.props.onDoubleClick(event)
  row.props.onDoubleClick(event)
  assert.equal(calls, 1)
  finish()
  await pending
  await new Promise(setImmediate)
  row.props.onDoubleClick(event)
  assert.equal(calls, 2)
})

test('proxy edit host waits for its profile before revealing the native window', () => {
  const ready = []
  const library = { loading: true, proxies: [], onReveal: (value) => ready.push(value) }
  const { ProxyFormHost } = loadComponent('proxies/proxy-form-host', {}, library)
  ProxyFormHost({ mode: 'edit', proxyId: proxy.id, standalone: true })
  assert.equal(ready.at(-1), false)
  library.loading = false
  library.proxies = [proxy]
  const form = ProxyFormHost({ mode: 'edit', proxyId: proxy.id, standalone: true })
  assert.equal(ready.at(-1), true)
  assert.equal(form.props.initialProxy.id, proxy.id)
})
