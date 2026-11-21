import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

// Render AddressLine with a minimal React shim so the assertions inspect the
// real element tree instead of the source text.
let copiedValues = []

function renderAddressLine({ value, masked }) {
  copiedValues = []
  const element = (type, props) => ({ type, props: { ...props } })
  const dependencies = {
    './system-resource-meters.css': {},
    react: {
      useCallback: (fn) => fn,
      useEffect: () => {},
      useRef: (current) => ({ current }),
      useState: (initial) => [typeof initial === 'function' ? initial() : initial, () => {}]
    },
    'react-dom': { createPortal: (node) => node },
    '@fileterm/core': {},
    '../../app/app-utils': { copyText: (text) => copiedValues.push(text), hasSelectedText: () => false },
    '../../i18n': {
      t: {
        connectionAddressMasked: 'Address hidden',
        copied: 'Copied',
        copy: 'Copy'
      },
      formatMessage: () => ''
    },
    '../common/dropdown-select': { DropdownSelect: () => null },
    '../common/vertical-scrollbar': { VerticalScrollbar: () => null },
    './system-metric-format': { formatSystemLoad: () => ({ value: '' }) }
  }
  // Compile the module to CommonJS with the automatic JSX runtime, then swap in
  // the shimmed runtime so no real React is needed.
  const source = readFileSync(
    new URL('../../src/renderer/features/system/system-resource-meters.tsx', import.meta.url),
    'utf8'
  )
  const output = ts.transpileModule(source, {
    compilerOptions: {
      jsx: ts.JsxEmit.ReactJSX,
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022
    }
  }).outputText
  dependencies['react/jsx-runtime'] = {
    Fragment: 'Fragment',
    jsx: (type, props) => element(type, props),
    jsxs: (type, props) => element(type, props)
  }
  const exports = {}
  vm.runInNewContext(output, {
    exports,
    module: { exports },
    // The component clears its "copied" flash on a timer.
    setTimeout: () => 0,
    clearTimeout: () => {},
    require: (name) => {
      assert.ok(name in dependencies, `Unexpected import ${name}`)
      return dependencies[name]
    }
  })
  const AddressLine = exports.AddressLine ?? exports.default?.AddressLine
  assert.equal(typeof AddressLine, 'function', 'AddressLine must be exported')
  return AddressLine({ label: 'IP', value, masked })
}

function textContent(node) {
  if (node == null) return ''
  if (typeof node === 'string') return node
  if (Array.isArray(node)) return node.map(textContent).join('')
  return textContent(node.props?.children)
}

test('masked addresses use a fixed redaction and keep the copy affordance', () => {
  const row = renderAddressLine({ value: '10.0.0.45', masked: true })
  const value = row.props.children[1]
  const maskedText = value.props.children.type(value.props.children.props)

  assert.equal(textContent(maskedText), '••••••••10.0.0.45')
  assert.match(value.props.className, /is-masked/)
  // The value stays in the accessibility tree as a copy target.
  assert.equal(value.props['aria-hidden'], undefined)
  assert.equal(value.props['aria-label'], 'Copy: 10.0.0.45')
  assert.match(value.props.className, /copyable/)
  assert.match(value.props.className, /is-masked/)
  // The value is revealed in place, so a native tooltip would only cover the row.
  assert.equal(value.props.title, undefined)
  assert.equal(maskedText.props.className, 'address-masked-text')
  assert.equal(maskedText.props.children[0].props.children, '••••••••')
  assert.equal(maskedText.props.children[0].props['aria-hidden'], 'true')
  assert.equal(maskedText.props.children[1].props.children, '10.0.0.45')
  assert.equal(maskedText.props.tabIndex, undefined)
  assert.equal(value.props.role, 'button')
  assert.equal(value.props.tabIndex, 0)
})

test('clicking a masked address copies the real value', () => {
  const row = renderAddressLine({ value: '10.0.0.45', masked: true })
  const value = row.props.children[1]

  assert.equal(copiedValues.length, 0)
  value.props.onClick()
  assert.deepEqual(copiedValues, ['10.0.0.45'])
})

test('unmasked addresses keep the copy affordance and the full value', () => {
  const row = renderAddressLine({ value: '10.0.0.45', masked: false })
  const value = row.props.children[1]

  assert.doesNotMatch(value.props.className, /is-masked/)
  assert.equal(value.props['aria-hidden'], undefined)
  assert.equal(value.props['aria-label'], 'Copy: 10.0.0.45')
  assert.equal(typeof value.props.onClick, 'function')
  value.props.onClick()
  assert.deepEqual(copiedValues, ['10.0.0.45'])
})

test('the empty placeholder stays inert in both states', () => {
  for (const masked of [true, false]) {
    const row = renderAddressLine({ value: '-', masked })
    const value = row.props.children[1]
    assert.doesNotMatch(value.props.className, /copyable/)
    value.props.onClick()
    assert.deepEqual(copiedValues, [])
  }
})

test('hover and keyboard focus reveal the value without a blur effect', () => {
  const css = readFileSync(
    new URL('../../src/renderer/features/system/system-resource-meters.css', import.meta.url),
    'utf8'
  )
  assert.match(css, /\.address-mask-placeholder/)
  assert.match(css, /\.address-row:hover strong\.is-masked \.address-mask-placeholder/)
  assert.match(css, /\.address-row strong\.is-masked:focus-visible \.address-mask-value/)
  assert.match(css, /\.system-description-row strong\.is-masked:focus-visible \.address-mask-value/)
  assert.match(css, /\.address-masked-text:focus-visible/)
  assert.match(css, /user-select:\s*none/)
  assert.doesNotMatch(css, /filter:\s*blur\(/)
})

test('system info and network views consume the address mask preference', () => {
  const systemInfo = readFileSync(
    new URL('../../src/renderer/features/system/system-info-workspace.tsx', import.meta.url),
    'utf8'
  )
  const network = readFileSync(
    new URL('../../src/renderer/features/system/network-connections-workspace.tsx', import.meta.url),
    'utf8'
  )
  const workspace = readFileSync(
    new URL('../../src/renderer/features/workspace/workspace-stage.tsx', import.meta.url),
    'utf8'
  )

  assert.match(systemInfo, /maskConnectionAddresses && row\.maskable/)
  assert.match(network, /maskConnectionAddresses \? <MaskedAddressText/)
  assert.match(workspace, /maskConnectionAddresses=\{maskConnectionAddresses\}/)
})
