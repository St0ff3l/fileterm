import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const moduleCache = new Map()
const directory = '../../src/renderer/features/terminal/autocomplete/'
function load(name, dependencies = {}, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(directory + name + '.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports, require: (name) => dependencies[name] ?? loadPure(name.replace('./', '')), ...globals }
  )
  return exports
}
function loadPure(name) {
  if (!moduleCache.has(name)) moduleCache.set(name, load(name))
  return moduleCache.get(name)
}
const model = { ...loadPure('completion-model'), ...loadPure('completion-input') }
const manifest = JSON.parse(readFileSync(new URL(directory + 'specs/index.json', import.meta.url), 'utf8'))
const commands = Object.entries(manifest.commands)
  .filter(([name]) => !name.includes('/'))
  .map(([value, entry]) => ({ value, description: entry.description }))
const loadSpec = async (name) => {
  const entry = manifest.commands[name]
  return entry ? JSON.parse(readFileSync(new URL(directory + 'specs/' + entry.file, import.meta.url), 'utf8')) : null
}
const complete = async (input, history = []) =>
  (await model.commandCompletions(input, history, 'History', commands, loadSpec)).items
const values = (items) => Array.from(items, (item) => item.value)
const screen = (before = 'user@host:~$ ', overrides = {}) => ({
  row: 1,
  before,
  after: '',
  normal: true,
  wrapped: false,
  atBottom: true,
  ...overrides
})

test('actual Fig rules provide Docker nested commands, scoped options and aliases rather than a hand-written shortlist', async () => {
  assert.ok(manifest.count > 1000)
  const docker = values(await complete('docker '))
  for (const name of ['compose', 'container', 'image', 'network', 'volume', 'build', 'exec', 'logs', 'system'])
    assert.ok(docker.includes('docker ' + name), name)
  assert.ok(values(await complete('docker')).includes('docker compose'))
  assert.ok(values(await complete('docker compose ')).includes('docker compose up'))
  assert.ok(values(await complete('docker compose')).includes('docker compose up'))
  assert.ok(values(await complete('docker container ')).includes('docker container inspect'))
  assert.ok(values(await complete('docker run --')).includes('docker run --publish'))
  assert.ok(values(await complete('docker compose up --')).includes('docker compose up --detach'))
  assert.ok(values(await complete('sudo docker compose up --')).includes('sudo docker compose up --detach'))
  assert.ok(
    values(await complete('env FOO=bar /usr/bin/docker logs --')).includes('env FOO=bar /usr/bin/docker logs --follow')
  )
  assert.ok(values(await complete('ls -l')).includes('ls -la') || values(await complete('ls -')).includes('ls -l'))
  const result = values(await complete('git st', ['git status', 'git stash', 'git stop\r']))
  assert.equal(result.filter((value) => value === 'git status').length, 1)
  assert.ok(!result.some((value) => value.includes('\r')))
})

test('quotes, escapes, pipelines and option values preserve the original input and argument context', async () => {
  assert.ok(values(await complete('echo ok && docker compose ')).includes('echo ok && docker compose up'))
  assert.ok(values(await complete('git status --')).includes('git status --short'))
  assert.ok(!values(await complete('docker logs --tail 10 --')).includes('docker logs --tail 10 --tail'))
  assert.ok(!values(await complete('docker logs --tail ')).some((value) => value.endsWith('--follow')))
  for (const input of ['echo $(', 'ls *', 'git\t']) assert.equal(model.completionContext(input), null)
  assert.equal(model.completionContext('cat "a b').token, 'a b')
  assert.equal(model.completionContext('cat a\\ b').token, 'a b')
  assert.deepEqual(values(model.pathCompletions('cat "a', '/home', [{ name: 'a b', type: 'file' }], 'Path')), [
    'cat "a b"'
  ])
  assert.equal(model.pathRequest('docker compose --file=/etc/co', '/home').path, '/etc')
})

test('remote paths normalize directory requests, restrict cd to directories, and escape shell metacharacters', () => {
  assert.equal(model.pathRequest('cd /us', '/home/me').path, '/')
  assert.equal(model.pathRequest('cat ../', '/home/me').path, '/home')
  const files = [
    { name: 'usr', type: 'folder' },
    { name: 'u-file', type: 'file' },
    { name: 'a $(whoami)', type: 'file' },
    { name: 'a\nfile', type: 'file' },
    { name: '.hidden', type: 'file' },
    { name: '中文', type: 'folder' }
  ]
  assert.deepEqual(values(model.pathCompletions('cd /u', '/home', files, 'Path', true)), ['cd /usr/'])
  assert.deepEqual(values(model.pathCompletions('cat a', '/home', files, 'Path')), ['cat a\\ \\$\\(whoami\\)'])
  assert.deepEqual(values(model.pathCompletions('cd 中', '/home', files, 'Path', true)), ['cd 中文/'])
  assert.ok(!values(model.pathCompletions('cat ', '/home', files, 'Path')).some((value) => value.includes('.hidden')))
})

test('input recovers ordinary prompts without OSC 7 and still requires matching echo and an end-of-line cursor', () => {
  const tracker = new model.CompletionInput()
  tracker.observe('g', screen())
  assert.equal(tracker.matches(screen('user@host:~$ g')), true)
  tracker.markPrompt()
  tracker.observe('g', screen())
  assert.equal(tracker.matches(screen()), false, 'unacknowledged remote echo cannot be completed')
  assert.equal(tracker.matches(screen('user@host:~$ g')), true)
  tracker.observe('it', screen('user@host:~$ g'))
  assert.equal(tracker.matches(screen('user@host:~$ git')), true)
  for (const overrides of [{ row: 2 }, { after: 'x' }, { normal: false }, { wrapped: true }, { atBottom: false }])
    assert.equal(tracker.matches(screen('user@host:~$ git', overrides)), false)
  tracker.observe('\x1b[D', screen('user@host:~$ git'))
  tracker.observe('x', screen('user@host:~$ gi'))
  assert.equal(tracker.matches(screen('user@host:~$ gix')), false)
})

test('passwords, pasted command lines and interactive programs never arm input tracking', () => {
  for (const data of ['git\r', '\x1b[200~git\x1b[201~', 'a\nb']) {
    const tracker = new model.CompletionInput()
    tracker.markPrompt()
    tracker.observe(data, screen())
    assert.equal(tracker.matches(screen('user@host:~$ ' + data)), false)
  }
  for (const prompt of ['Password: ', 'PIN: ', 'Verification code: ', 'mysql> ', '>>> ']) {
    const guarded = new model.CompletionInput()
    guarded.markPrompt()
    guarded.observe('g', screen(prompt))
    assert.equal(guarded.input, '', prompt)
  }
  const tracker = new model.CompletionInput()
  tracker.markPrompt()
  tracker.observe('g', screen('Password: '))
  assert.equal(tracker.input, '')
  tracker.markPrompt()
  tracker.observe('g', screen())
  tracker.observe('i', screen('Password: '))
  assert.equal(tracker.input, '')
})

const flush = async () => {
  for (let index = 0; index < 32; index++) await Promise.resolve()
}
async function fixture(initialBefore = '$ ') {
  const callbacks = {}
  const textarea = new EventTarget()
  const controllerRef = { current: null }
  const composing = { current: false }
  const replaying = { current: false }
  const buffer = {
    baseY: 0,
    cursorY: 1,
    cursorX: 2,
    viewportY: 0,
    type: 'normal',
    getLine: () => ({ isWrapped: false, translateToString: (trim, start) => (start === 0 ? before : after) })
  }
  let before = initialBefore
  let after = ''
  let popup = null
  let cleanup
  let nextTimer = 0
  const timers = new Map()
  const sent = []
  const pendingPaths = []
  const document = { activeElement: textarea }
  const terminal = {
    buffer: { active: buffer },
    rows: 24,
    cols: 80,
    textarea,
    element: { querySelector: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 480 }) }) },
    parser: {
      registerOscHandler: (id, fn) => {
        callbacks.prompt = fn
        return { dispose() {} }
      }
    },
    onWriteParsed: (fn) => {
      callbacks.parsed = fn
      return { dispose() {} }
    },
    onScroll: (fn) => {
      callbacks.scroll = fn
      return { dispose() {} }
    },
    onResize: (fn) => {
      callbacks.resize = fn
      return { dispose() {} }
    },
    input: (text) => {
      sent.push(text)
      controllerRef.current.input(text)
    },
    focus() {}
  }
  const api = {
    getSnapshot: async () => ({
      profiles: [{ id: 'profile', type: 'ssh', terminalCommandCompletion: true }],
      sessions: { tab: { connected: true, shellCwd: '/home/me', remotePath: '/home/me', remoteFiles: [] } }
    }),
    onWorkspaceSnapshot: async (fn) => {
      callbacks.snapshot = fn
      callbacks.completionProfile = (preferences) =>
        fn({
          profiles: [{ id: 'profile', type: 'ssh', terminalCommandCompletion: preferences.terminalCommandCompletion }],
          sessions: { tab: { connected: true, shellCwd: '/home/me', remotePath: '/home/me', remoteFiles: [] } }
        })
      return () => {}
    },
    getTerminalCommandHistory: async () => [{ command: 'git stash' }],
    listRemoteDirectory: (tabId, path) =>
      new Promise((resolve, reject) => pendingPaths.push({ tabId, path, resolve, reject }))
  }
  const window = Object.assign(new EventTarget(), { fileterm: api, innerWidth: 1000, innerHeight: 600 })
  const hook = load(
    'use-terminal-completion',
    {
      react: {
        useState: () => [
          null,
          (next) => {
            popup = next
          }
        ],
        useRef: (current) => ({ current }),
        useEffect: (effect) => {
          cleanup = effect()
        }
      },
      './completion-model': model,
      './completion-input': loadPure('completion-input'),
      './fig-spec-loader': { FIG_COMMANDS: commands, loadCommandSpec: loadSpec },
      '../../../i18n': { t: { terminalCompletionHistory: 'History', terminalCompletionPath: 'Path' } }
    },
    {
      window,
      document,
      setTimeout: (fn) => {
        timers.set(++nextTimer, fn)
        return nextTimer
      },
      clearTimeout: (id) => timers.delete(id)
    }
  )
  const result = hook.useTerminalCompletion({
    terminalRef: { current: terminal },
    controllerRef,
    tabId: 'tab',
    profileId: 'profile',
    sessionType: 'ssh',
    active: true,
    available: true,
    blocked: false,
    composing,
    replaying
  })
  await flush()
  const echo = (text, tail = '') => {
    before = '$ ' + text
    after = tail
    buffer.cursorX = before.length
    callbacks.parsed()
  }
  return {
    callbacks,
    viewport: window,
    composing,
    replaying,
    sent,
    pendingPaths,
    terminal,
    document,
    textarea,
    cleanup,
    get popup() {
      return popup
    },
    key: (key, overrides = {}) => controllerRef.current.key({ type: 'keydown', key, ...overrides }),
    input: (data) => controllerRef.current.input(data),
    prompt: () => {
      before = '$ '
      after = ''
      buffer.cursorX = 2
      callbacks.prompt('file:///home/me')
      callbacks.parsed()
    },
    echo,
    accept: result.acceptCompletion,
    select: result.selectCompletion,
    runTimers: () => {
      const pending = Array.from(timers.values())
      timers.clear()
      pending.forEach((fn) => fn())
    }
  }
}

test('an already visible Docker prompt works without a fresh OSC marker, survives unchanged profile broadcasts and resize', async () => {
  const f = await fixture('$ docker ')
  await flush()
  assert.ok(f.popup.items.some((item) => item.value === 'docker compose'))
  f.callbacks.completionProfile({ terminalCommandCompletion: true, terminalRenderer: 'dom' })
  await flush()
  assert.ok(f.popup.items.some((item) => item.value === 'docker container'))
  f.callbacks.resize()
  await flush()
  assert.ok(f.popup.items.some((item) => item.value === 'docker run'))
  assert.equal(f.key('Tab'), true)
  assert.ok(f.sent[0] && !f.sent[0].includes('\r'))
  f.cleanup()
})

test('keyboard selection and mouse acceptance insert suffix only, and empty results preserve remote Tab', async () => {
  const f = await fixture()
  f.prompt()
  f.input('git ')
  f.echo('git ')
  await flush()
  assert.ok(f.popup.items.length > 3)
  f.select(2)
  assert.equal(f.popup.selected, 2, 'pointer movement updates the same selected candidate used by the keyboard')
  f.select(999)
  assert.equal(f.popup.selected, 2, 'invalid pointer targets cannot change selection')
  assert.equal(f.key('ArrowDown'), true)
  assert.equal(f.popup.selected, 3, 'keyboard navigation continues from the pointer-selected candidate')
  const selected = f.popup.items[f.popup.selected].value
  assert.equal(f.key('Tab'), true)
  assert.deepEqual(f.sent, [selected.slice(4)])
  assert.ok(!f.sent.some((value) => /[\r\n]/.test(value)))
  f.echo(selected)
  await flush()
  f.prompt()
  f.input('git st')
  f.echo('git st')
  await flush()
  const clicked = f.popup.items[0].value
  f.accept(0)
  assert.equal(f.sent.at(-1), clicked.slice('git st'.length))
  f.prompt()
  f.input('zzzz')
  f.echo('zzzz')
  await flush()
  assert.equal(f.popup, null)
  assert.equal(f.key('Tab'), false)
  f.cleanup()
})

test('completion refuses stale echo, composing confirmation, password prompts, alternate screens and disabled settings', async () => {
  const f = await fixture()
  f.prompt()
  f.input('g')
  f.echo('g')
  await flush()
  f.input('i')
  assert.equal(f.key('Tab'), false)
  assert.deepEqual(f.sent, [])
  f.echo('gi')
  await flush()
  f.composing.current = true
  assert.equal(f.key('Tab', { isComposing: true }), false)
  f.composing.current = false
  f.echo('gi')
  await flush()
  f.callbacks.completionProfile({ terminalCommandCompletion: false })
  assert.equal(f.popup, null)
  assert.equal(f.key('ArrowDown'), false)
  assert.equal(f.key('Tab'), false, 'disabled completion leaves Tab to the remote shell')
  assert.equal(f.key('Escape'), false)
  f.input('t')
  f.echo('git')
  await flush()
  assert.equal(f.popup, null, 'typing cannot reopen FileTerm candidates while disabled')
  f.callbacks.completionProfile({ terminalCommandCompletion: true })
  await flush()
  assert.ok(f.popup, 'enabling takes effect in the current prompt without reconnecting')
  f.prompt()
  f.input('g')
  f.echo('g')
  await flush()
  f.terminal.buffer.active.type = 'alternate'
  f.callbacks.parsed()
  assert.equal(f.popup, null)
  assert.equal(f.key('Tab'), false)
  f.terminal.buffer.active.type = 'normal'
  f.echo('Password: ')
  await flush()
  f.input('g')
  f.echo('Password: g')
  await flush()
  assert.equal(f.popup, null)
  f.cleanup()
})

test('directory errors preserve local suggestions and delayed path results cannot repopulate a dismissed popup', async () => {
  const f = await fixture()
  f.prompt()
  f.input('git add ')
  f.echo('git add ')
  await flush()
  f.runTimers()
  assert.equal(f.pendingPaths.length, 1)
  f.pendingPaths[0].reject(new Error('SFTP unavailable'))
  await flush()
  assert.ok(f.popup.items.some((item) => item.value.startsWith('git add -')))
  f.prompt()
  f.input('cd /u')
  f.echo('cd /u')
  await flush()
  f.runTimers()
  assert.equal(f.pendingPaths.at(-1).path, '/')
  f.input('s')
  f.echo('cd /us')
  await flush()
  f.pendingPaths.at(-1).resolve([{ name: 'usr', type: 'folder' }])
  await flush()
  assert.equal(f.popup, null)
  f.runTimers()
  assert.equal(f.pendingPaths.length, 3, 'the newest input retries once the earlier request finishes')
  f.pendingPaths.at(-1).resolve([{ name: 'usr', type: 'folder' }])
  await flush()
  assert.deepEqual(values(f.popup.items), ['cd /usr/'])
  assert.equal(f.key('Escape'), true)
  assert.equal(f.key('Tab'), false)
  f.cleanup()
})

test('panes do not share input, selected candidates or pending directory results', async () => {
  const left = await fixture()
  const right = await fixture()
  left.prompt()
  left.input('git ')
  left.echo('git ')
  await flush()
  right.prompt()
  right.input('ls -')
  right.echo('ls -')
  await flush()
  left.key('ArrowDown')
  assert.equal(right.popup.selected, 0)
  right.key('Tab')
  assert.deepEqual(left.sent, [])
  assert.equal(right.sent.length, 1)
  left.cleanup()
  right.cleanup()
})

test('the data conversion never executes generators and preserves the Fig license and pinned version', async () => {
  const { staticSpec } = await import('../../scripts/sync-completion-specs.mjs')
  const fail = () => {
    throw new Error('Generator must never execute')
  }
  const result = staticSpec({
    name: 'fixture',
    args: { template: 'folders', generators: { script: ['rm', '-rf', '/'], custom: fail } },
    subcommands: [{ name: 'nested', options: [{ name: ['-a', '--all'], args: { suggestions: ['one', 'two'] } }] }],
    loadSpec: fail
  })
  assert.equal(result.args.template, 'folders')
  assert.equal(result.args.generators, undefined)
  assert.equal(result.loadSpec, undefined)
  assert.equal(result.subcommands[0].options[0].name[1], '--all')
  const pkg = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'))
  assert.equal(manifest.version, pkg.devDependencies['@withfig/autocomplete'])
  const license = readFileSync(new URL(directory + 'specs/LICENSE.fig.txt', import.meta.url), 'utf8')
  assert.ok(license.includes('MIT License') && license.includes('Hercules Labs Inc. (Fig)'))
  const notices = readFileSync(new URL('../../../../THIRD-PARTY-NOTICES.md', import.meta.url), 'utf8')
  assert.ok(notices.includes(manifest.version) && notices.includes(license.trim()))
})

test('wrapped candidate descriptions use available viewport height and anchor above the cursor near the bottom', async () => {
  const f = await fixture('$ docker ')
  await flush()
  assert.equal(f.popup.above, false)
  assert.equal(f.popup.height, 320)
  assert.ok(f.popup.top + f.popup.height <= f.viewport.innerHeight - 8)
  f.terminal.buffer.active.cursorY = 22
  f.callbacks.resize()
  await flush()
  assert.equal(f.popup.above, true)
  assert.ok(f.popup.top - f.popup.height >= 8)
  assert.equal(f.popup.top, (22 * 480) / 24 - 4)
  f.viewport.innerWidth = 360
  f.viewport.innerHeight = 480
  f.callbacks.resize()
  await flush()
  assert.ok(f.popup.left >= 8)
  assert.ok(f.popup.left + f.popup.width <= 352)
  f.cleanup()
})

test('completion follows the current connection, ignoring another connection setting', async () => {
  const f = await fixture('$ docker ')
  await flush()
  f.callbacks.snapshot({
    profiles: [
      { id: 'profile', type: 'ssh' },
      { id: 'other', type: 'ssh', terminalCommandCompletion: false }
    ],
    sessions: { tab: { connected: true, shellCwd: '/home/me', remotePath: '/home/me', remoteFiles: [] } }
  })
  await flush()
  assert.ok(f.popup, 'legacy connections default to enabled independently of other connections')
  f.callbacks.completionProfile({ terminalCommandCompletion: false })
  assert.equal(f.popup, null)
  assert.equal(f.key('Tab'), false)
  f.cleanup()
})
