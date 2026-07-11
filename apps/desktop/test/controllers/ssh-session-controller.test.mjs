import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import test from 'node:test'
import { LiveSshSessionController } from '../../dist-electron/main/services/sessions/ssh-session-controller.js'

class FakeChannel extends EventEmitter {
  stderr = new EventEmitter()
  writes = []
  windowSizes = []
  ended = false
  destroyed = false

  write(data) {
    this.writes.push(String(data))
    return true
  }

  setWindow(rows, cols, height, width) {
    this.windowSizes.push({ rows, cols, height, width })
  }

  end() {
    this.ended = true
  }

  destroy() {
    this.destroyed = true
  }
}

class FakeSshClient extends EventEmitter {
  channel = new FakeChannel()
  connectCalls = []
  shellOptions = []
  execCommands = []
  endCalls = 0

  constructor(execResponder = () => '') {
    super()
    this.execResponder = execResponder
  }

  connect(config) {
    this.connectCalls.push(config)
    queueMicrotask(() => this.emit('ready'))
    return this
  }

  shell(options, callback) {
    this.shellOptions.push(options)
    callback(undefined, this.channel)
  }

  exec(command, optionsOrCallback, maybeCallback) {
    const callback = typeof optionsOrCallback === 'function' ? optionsOrCallback : maybeCallback
    const channel = new FakeChannel()
    this.execCommands.push(command)
    callback(undefined, channel)
    queueMicrotask(() => {
      const response = this.execResponder(command)
      if (response) {
        channel.emit('data', Buffer.from(response))
      }
      channel.emit('close', 0)
    })
  }

  end() {
    this.endCalls += 1
  }
}

function createProfile(overrides = {}) {
  return {
    id: 'ssh-profile',
    name: 'SSH test',
    type: 'ssh',
    host: 'example.test',
    port: 22,
    group: '',
    username: 'tester',
    authType: 'password',
    password: 'secret',
    sftpEnabled: true,
    remotePath: '/home/tester',
    enableExecChannel: false,
    ...overrides
  }
}

function createController(profile, clients, callbacks = {}) {
  return new LiveSshSessionController(
    'ssh-tab',
    profile,
    async () => ({ kind: 'host-verification', decision: 'accept-once' }),
    async () => {},
    callbacks.onData ?? (() => {}),
    callbacks.onCwd ?? (() => {}),
    callbacks.onUser ?? (() => {}),
    callbacks.onState ?? (() => {}),
    undefined,
    clients
  )
}

async function waitFor(predicate, timeoutMs = 500) {
  const deadline = Date.now() + timeoutMs
  while (!predicate()) {
    if (Date.now() >= deadline) {
      throw new Error('Timed out waiting for fake SSH activity')
    }
    await new Promise((resolve) => setTimeout(resolve, 2))
  }
}

test('SSH controller owns shell lifecycle, pending resize, input, output and disconnect', async () => {
  const main = new FakeSshClient()
  const exec = new FakeSshClient()
  const sftp = new FakeSshClient()
  const transfer = new FakeSshClient()
  const output = []
  const states = []
  const controller = createController(
    createProfile(),
    { main, exec, sftp, transfer, createTransferClient: () => new FakeSshClient() },
    {
      onData: (chunk) => output.push(chunk),
      onState: (summary, _transcript, connected) => states.push({ summary, connected })
    }
  )

  await controller.resize(140, 40, 1000, 700)
  await controller.connect()
  assert.equal(controller.getSummary(), 'Connected to example.test:22')
  assert.deepEqual(main.shellOptions, [{ term: 'xterm-256color', rows: 40, cols: 140 }])
  assert.deepEqual(main.channel.windowSizes, [{ rows: 40, cols: 140, height: 700, width: 1000 }])

  await controller.write('echo ok\r')
  assert.deepEqual(main.channel.writes, ['echo ok\r'])
  main.channel.emit('data', Buffer.from('ok\r\n'))
  assert.equal(output.at(-1), 'ok\r\n')
  assert.equal(
    states.some((state) => state.connected),
    true
  )

  await controller.disconnect()
  assert.equal(main.channel.ended, true)
  assert.equal(main.endCalls, 1)
  assert.equal(exec.endCalls, 1)
  assert.equal(sftp.endCalls, 1)
  assert.equal(transfer.endCalls, 1)
  assert.equal(controller.getSummary(), 'Ready to connect example.test:22')
})

test('SSH controller does not inject POSIX setup after Windows platform detection', async () => {
  const main = new FakeSshClient()
  const exec = new FakeSshClient((command) => {
    if (command.includes('__FILETERM_PROBE_START__')) {
      return '__FILETERM_PROBE_START__\nnot-posix\n__FILETERM_PROBE_END__\n'
    }
    if (command.includes('OSVersion.Platform')) {
      return 'Win32NT\r\n'
    }
    return 'Microsoft Windows\r\n'
  })
  const controller = createController(createProfile({ enableExecChannel: true }), {
    main,
    exec,
    sftp: new FakeSshClient(),
    transfer: new FakeSshClient()
  })

  await controller.connect()
  main.channel.emit('data', Buffer.from('C:\\Users\\tester>'))
  await waitFor(() => exec.execCommands.some((command) => command.includes('OSVersion.Platform')))
  await new Promise((resolve) => setImmediate(resolve))

  assert.equal(
    main.channel.writes.some((value) => value.includes('__tdcwd') || value.includes('test -z')),
    false
  )
  await controller.disconnect()
})

test('SSH controller injects shell setup only after a confirmed Linux probe', async () => {
  const main = new FakeSshClient()
  const exec = new FakeSshClient((command) =>
    command.includes('__FILETERM_PROBE_START__') ? '__FILETERM_PROBE_START__\nLinux\n__FILETERM_PROBE_END__\n' : ''
  )
  const controller = createController(createProfile({ enableExecChannel: true }), {
    main,
    exec,
    sftp: new FakeSshClient(),
    transfer: new FakeSshClient()
  })

  await controller.connect()
  main.channel.emit('data', Buffer.from('tester@linux:~$ '))
  await waitFor(() => main.channel.writes.some((value) => value.includes('__tdcwd')))

  assert.equal(
    main.channel.writes.some((value) => value.includes('test -z "${FISH_VERSION-}"')),
    true
  )
  await controller.disconnect()
})
