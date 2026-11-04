import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'
import * as react from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as jsxRuntime from 'react/jsx-runtime'

function loadComponent(path, dependencies) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    {
      exports,
      require(name) {
        if (name === 'react') return dependencies.react ?? react
        if (name.endsWith('.css')) return {}
        if (name === 'react/jsx-runtime') return jsxRuntime
        assert.ok(name in dependencies, `Unexpected dependency: ${name}`)
        return dependencies[name]
      }
    }
  )
  return exports
}

const labels = {
  sshSecondFactor: '二次验证（可选）',
  otpSecret: 'OTP 密钥',
  savedSecretPlaceholder: '留空保留已保存的{label}',
  secretPlaceholder: '输入{label}',
  showSecret: '显示{label}',
  clearSavedSecret: '清除已保存的{label}',
  secretMarkedForClear: '保存时将清除已保存的{label}'
}
const t = new Proxy(labels, { get: (target, key) => target[key] ?? key })
const { ConnectionSecretField } = loadComponent('../src/renderer/features/connections/connection-secret-field.tsx', {
  '../../i18n': { t },
  '../common/app-icon': { AppIcon: () => null }
})
const { ConnectionSshAuthSection } = loadComponent(
  '../src/renderer/features/connections/connection-ssh-auth-section.tsx',
  {
    '../../i18n': { t },
    '../../components/common/material-icon/material-icon': { MaterialIcon: () => null },
    '../common/app-icon': { AppIcon: () => null },
    '../common/dropdown-select': { DropdownSelect: () => null },
    './connection-secret-field': { ConnectionSecretField },
    './ssh-private-key-field': { SshPrivateKeyField: () => null },
    './connection-modal-utils': loadComponent('../src/renderer/features/connections/connection-modal-utils.ts', {})
  }
)

function descendants(element) {
  if (!react.isValidElement(element)) return []
  return [element, ...react.Children.toArray(element.props.children).flatMap(descendants)]
}

test('OTP settings preserve saved keys, support replacing and clearing, and retain custom prompt text', () => {
  let form = { type: 'ssh', authType: 'password', username: 'alice', otpSecret: '', otpPrompt: 'company token' }
  const tree = ConnectionSshAuthSection({
    form,
    connectionDefaults: {},
    mode: 'edit',
    hasSavedOtpSecret: true,
    setForm(update) {
      form = update(form)
    }
  })
  const elements = descendants(tree)
  const secret = elements.find((element) => element.props.id === 'connection-otp-secret')
  assert.ok(secret)
  const sections = elements.filter((element) => element.type === 'fieldset')
  assert.equal(sections.length, 2)
  assert.ok(!descendants(sections[0]).some((element) => element.props.id === 'connection-otp-secret'))
  assert.ok(descendants(sections[1]).some((element) => element.props.id === 'connection-otp-secret'))
  assert.match(renderToStaticMarkup(sections[1]), /<legend>二次验证（可选）<\/legend>/)
  const html = renderToStaticMarkup(tree)
  assert.match(html, /id="connection-otp-secret"[^>]*type="password"[^>]*value=""/)
  assert.match(html, /留空保留已保存的OTP 密钥/)
  assert.match(html, /aria-label="显示OTP 密钥"/)
  const promptField = elements.find((element) => element.props.className === 'ssh-form-field span-2')
  assert.ok(descendants(promptField).some((element) => element.props.htmlFor === 'connection-otp-prompt'))
  assert.doesNotMatch(html, /输入密码/)
  assert.match(html, /id="connection-otp-prompt"[^>]*value="company token"/)
  secret.props.onChange('JBSWY3DPEHPK3PXP')
  assert.equal(form.otpSecret, 'JBSWY3DPEHPK3PXP')
  secret.props.onClear()
  assert.equal(form.otpSecret, null)
  secret.props.onUndo()
  assert.equal(form.otpSecret, '')
  const prompt = elements.find((element) => element.props.id === 'connection-otp-prompt')
  prompt.props.onChange({ target: { value: 'Authenticator:' } })
  assert.equal(form.otpPrompt, 'Authenticator:')
})

test('OTP settings are only available for SSH profiles', () => {
  const tree = ConnectionSshAuthSection({ form: { type: 'ftp' }, mode: 'create' })
  assert.ok(!descendants(tree).some((element) => element.props.id === 'connection-otp-secret'))
})

test('credential hints follow the authentication method without treating an empty field as an empty password', () => {
  for (const [authType, usernameHint, passwordHint] of [
    ['password', 'sshUsernameOnConnectHint', 'sshPasswordOnConnectHint'],
    ['kubernetes', 'sshUsernameOnConnectHint', 'sshPasswordOnConnectHint'],
    ['keyboard-interactive', 'sshUsernameRequiredHint', 'sshKeyboardPasswordHint'],
    ['jumpserver-koko-mfa', 'sshUsernameRequiredHint', 'sshKokoPasswordHint'],
    ['privateKey', 'sshUsernameRequiredHint', undefined],
    ['system', 'sshUsernameRequiredHint', undefined],
    ['sshConfig', 'sshConfigUsernamePlaceholder', undefined]
  ]) {
    const tree = ConnectionSshAuthSection({
      form: { type: 'ssh', authType },
      connectionDefaults: {},
      mode: 'create'
    })
    const elements = descendants(tree)
    const username = elements.find((element) => element.props.id === 'connection-username-annotation')
    const password = elements.find((element) => element.props.id === 'connection-password')
    assert.equal(username.props.title, usernameHint, authType)
    assert.equal(password?.props.annotationTitle, passwordHint, authType)
  }
  const elements = descendants(
    ConnectionSshAuthSection({
      form: { type: 'ssh', authType: 'password', useEmptyPassword: true },
      connectionDefaults: {},
      mode: 'create'
    })
  )
  assert.equal(
    elements.find((element) => element.props.id === 'connection-password').props.annotationTitle,
    'useEmptyPasswordHint'
  )
})

test('connection target hints are contextual and low-resource help expands separately from its controls', () => {
  let shown = false
  const { ConnectionSshGeneralSection } = loadComponent(
    '../src/renderer/features/connections/connection-ssh-general-section.tsx',
    {
      react: {
        ...react,
        useState: () => [
          shown,
          (update) => {
            shown = update(shown)
          }
        ]
      },
      '@fileterm/shared': { normalizeConnectionHost: (host) => host },
      '../../i18n': { t },
      '../../components/common/button/button': { Button: () => null },
      '../../components/common/segmented-control/segmented-control': { SegmentedControl: () => null },
      '../common/app-icon': { AppIcon: () => null },
      '../common/dropdown-select': { DropdownSelect: () => null },
      '../common/selection-control': { SelectionControl: () => null },
      '../common/stable-button-content': { StableButtonContent: () => null },
      './connection-low-resource-mode': loadComponent(
        '../src/renderer/features/connections/connection-low-resource-mode.ts',
        {}
      ),
      './ssh-config-host-field': { SshConfigHostField: () => null }
    }
  )
  const render = (deviceMode = 'server') =>
    descendants(
      ConnectionSshGeneralSection({
        form: { type: 'ssh', deviceMode },
        connectionDefaults: {},
        groupOptions: [],
        isNetworkDevice: deviceMode === 'network-device',
        setForm() {}
      })
    )
  let elements = render()
  assert.ok(!elements.some((element) => element.props.children === 'sshDeviceModeHint'))
  assert.ok(!elements.some((element) => element.props.children === 'sshDeviceModeAutoHint'))
  const details = () => elements.find((element) => element.props.id === 'connection-low-resource-mode-details')
  assert.equal(details().props.hidden, true)
  elements.find((element) => element.props['aria-controls'] === 'connection-low-resource-mode-details').props.onClick()
  elements = render()
  assert.equal(details().props.hidden, false)
  const controls = elements.find((element) => element.props.className === 'ssh-low-resource-mode__controls')
  assert.ok(!descendants(controls).some((element) => element.props.id === 'connection-low-resource-mode-details'))
  assert.ok(render('network-device').some((element) => element.props.children === 'sshDeviceModeHint'))
  assert.ok(render('auto').some((element) => element.props.children === 'sshDeviceModeAutoHint'))
})
