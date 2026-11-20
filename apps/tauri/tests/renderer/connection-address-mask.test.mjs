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

test('masked addresses blur the value but keep the copy affordance', () => {
  const row = renderAddressLine({ value: '10.0.0.45', masked: true })
  const value = row.props.children[1]

  assert.equal(textContent(row), 'IP10.0.0.45')
  assert.match(value.props.className, /is-masked/)
  // The value stays in the accessibility tree as a copy target; only the paint
  // is blurred, so a screen reader still announces what can be copied.
  assert.equal(value.props['aria-hidden'], undefined)
  assert.equal(value.props['aria-label'], 'Copy: 10.0.0.45')
  assert.match(value.props.className, /copyable/)
  assert.match(value.props.className, /is-masked/)
  // No native tooltip: the value is visible while hovered anyway, and a bubble
  // would only obscure the row.
  assert.equal(value.props.title, undefined)
  // The blur is painted on a wrapper that carries no floating full-value tooltip.
  assert.equal(value.props.children.props.className, 'address-masked-text')
  assert.equal(value.props.children.props.title, undefined)
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

test('hovering clears the blur and the value stays unselectable', () => {
  const css = readFileSync(
    new URL('../../src/renderer/features/system/system-sidebar-controls.css', import.meta.url),
    'utf8'
  )
  const maskedRule = css.slice(css.indexOf('strong.is-masked'))
  assert.ok(maskedRule, 'the masked rule must exist')
  // Drag selection would leak the characters through the clipboard.
  assert.match(maskedRule.split('}')[0], /user-select:\s*none/)
  // Resting the pointer on the row removes the blur entirely, including when the
  // pointer is over the label rather than the value itself.
  const hoverRule = css.match(/strong\.is-masked:hover[^{]*\{([^}]*)\}/)
  assert.ok(hoverRule, 'a hover reveal rule must exist')
  assert.match(hoverRule[1], /filter:\s*blur\(0\)/)
  assert.match(hoverRule[1], /opacity:\s*1/)
  assert.match(css, /\.address-row:hover strong\.is-masked/, 'the whole row is a hover target')
  // Keyboard users get the same reveal as a pointer hover.
  assert.match(css, /strong\.is-masked:focus-visible[^{]*\{/)
  // A successful copy keeps the green confirmation readable.
  assert.match(css, /strong\.is-masked\.copied[^{]*\{[^}]*color:\s*var\(--status-success\)/)
  // The default state is still blurred.
  assert.match(css, /\.address-masked-text[\s\S]*?filter:\s*blur\(/)
})
