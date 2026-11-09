import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const source = readFileSync(new URL('../../src/renderer/i18n.ts', import.meta.url), 'utf8')
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  { exports, URLSearchParams }
)

test('localizes detailed SSH handshake errors inside hop failures', () => {
  const raw =
    'SSH hop 1 (Windows xlight ssh): SSH connect failed: DH prime size (1024 bits) not within requested range (3072-8192 bits)'

  exports.setLocale('zhCN')
  assert.equal(
    exports.localizeConnectionErrorText(raw),
    'SSH hop 1 (Windows xlight ssh): SSH 连接失败：服务器提供的 DH 素数为 1024 位，超出本连接请求的 3072–8192 位范围。请调整服务器 DH 参数，或对可信的旧服务器启用“兼容旧服务器算法”。'
  )

  exports.setLocale('enUS')
  assert.equal(
    exports.localizeConnectionErrorText(raw),
    'SSH hop 1 (Windows xlight ssh): SSH connection failed: The server offered a 1024-bit DH prime, outside this connection’s requested 3072–8192-bit range. Update the server DH parameters, or enable “Legacy server algorithms” for a trusted old server.'
  )
})

test('localizes disconnects inside SSH hop failures', () => {
  exports.setLocale('zhCN')
  assert.equal(
    exports.localizeConnectionErrorText('SSH hop 1 (Windows xlight ssh): SSH connect failed: Disconnected'),
    'SSH hop 1 (Windows xlight ssh): SSH 服务器在握手阶段主动断开连接，请等待几秒后再试；若仍失败请检查服务器连接限制。'
  )
})

test('adds actionable diagnostics for common connection failures and keeps raw details', () => {
  exports.setLocale('zhCN')

  const refused = exports.localizeConnectionErrorText(
    'FTP connect failed: host="192.0.2.4" port=21: Connection refused (os error 10061)'
  )
  assert.match(refused, /目标服务器或代理拒绝了 TCP 连接/)
  assert.match(refused, /192\.0\.2\.4.*10061/)

  const dns = exports.localizeConnectionErrorText(
    'Telnet connect failed: host="bad.example" port=23: failed to lookup address information: No such host is known (os error 11001)'
  )
  assert.match(dns, /无法解析目标或代理的主机名/)
  assert.match(dns, /bad\.example.*11001/)

  const timeout = exports.localizeConnectionErrorText(
    'FTP connect/login timed out: host="192.0.2.4" port=21 after 30 seconds'
  )
  assert.match(timeout, /连接等待响应超时/)
  assert.match(timeout, /30 seconds/)

  const authentication = exports.localizeConnectionErrorText(
    'SSH Authentication failed at 192.0.2.4:2222 (configured method: password)'
  )
  assert.match(authentication, /身份验证被服务器拒绝/)
  assert.match(authentication, /认证方式：password/)

  assert.match(
    exports.localizeConnectionErrorText('FTP login failed: Invalid response: [530] Login incorrect'),
    /服务器拒绝了身份验证/
  )
  assert.match(
    exports.localizeConnectionErrorText('FTPS TLS handshake failed: certificate verify failed'),
    /TLS\/证书协商失败/
  )
  assert.match(exports.localizeConnectionErrorText('SSH connect failed: Key exchange init failed'), /SSH 握手未完成/)
  const algorithmMismatch = exports.localizeConnectionErrorText(
    'SSH connect failed: No common Kex algorithm - ours: ["curve25519-sha256"], theirs: ["diffie-hellman-group1-sha1"]'
  )
  assert.match(algorithmMismatch, /没有共同支持的算法/)
  assert.match(algorithmMismatch, /curve25519-sha256.*diffie-hellman-group1-sha1/)
  assert.match(
    exports.localizeConnectionErrorText('FTP control handshake failed: unexpected frame'),
    /FTP、显式 FTPS 或隐式 FTPS/
  )
  assert.match(
    exports.localizeConnectionErrorText('FTP connect failed: an unfamiliar transport error'),
    /请核对所选协议、主机\/IP、端口/
  )

  exports.setLocale('enUS')
  assert.match(
    exports.localizeConnectionErrorText('Telnet connect failed: Connection refused (os error 10061)'),
    /target server or proxy refused the TCP connection/i
  )
})

test('connection test errors keep their diagnosis when the peer closed the connection', () => {
  const appErrorExports = {}
  const appErrorSource = readFileSync(new URL('../../src/renderer/app/app-error-utils.ts', import.meta.url), 'utf8')
  vm.runInNewContext(
    ts.transpileModule(appErrorSource, {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports: appErrorExports,
      require(name) {
        if (name === '../i18n') return exports
        if (name === './app-shell-utils') return { REMOTE_METHOD_ERROR_PREFIX: '' }
        throw new Error(`Unexpected import ${name}`)
      }
    }
  )

  exports.setLocale('zhCN')
  const message = appErrorExports.formatAppError('测试连接', 'FTP connect failed: connection closed by server', 'zhCN')
  assert.match(message, /服务器或代理在连接过程中关闭\/重置了连接/)
  assert.match(message, /FTP 连接失败/)

  const shareMessage = appErrorExports.formatAppError(
    '连接 SMB 网络共享',
    'SMB 连接失败（Windows 错误码 53），Connection closed by server',
    'zhCN'
  )
  assert.match(shareMessage, /服务器或代理在连接过程中关闭\/重置了连接/)
  assert.match(shareMessage, /Windows 错误码 53/)
})

test('terminal localization enriches FileTerm connection notices without changing remote output', () => {
  const terminalExports = {}
  const terminalSource = readFileSync(
    new URL('../../src/renderer/components/terminal-view-utils.ts', import.meta.url),
    'utf8'
  ).replaceAll('import.meta.env.DEV', 'false')
  vm.runInNewContext(
    ts.transpileModule(terminalSource, {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports: terminalExports,
      require(name) {
        if (name === '../app/terminal-transcript') return { stripClinkAutosuggestPrompt: (value) => value }
        if (name === '../i18n') return exports
        throw new Error(`Unexpected import ${name}`)
      }
    }
  )

  exports.setLocale('zhCN')
  const internal = '连接失败: FTP connect failed: Connection refused (os error 10061)'
  assert.match(terminalExports.localizeTerminalText(internal), /目标服务器或代理拒绝了 TCP 连接/)

  const remoteOutput = 'curl: (7) Failed to connect: Connection refused'
  assert.equal(terminalExports.localizeTerminalText(remoteOutput), remoteOutput)
})
