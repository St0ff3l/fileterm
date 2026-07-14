import assert from 'node:assert/strict'
import { once } from 'node:events'
import { EventEmitter } from 'node:events'
import net from 'node:net'
import { PassThrough } from 'node:stream'
import test from 'node:test'
import { createOutboundSocket } from '../../dist-electron/main/services/network/proxy-socket-factory.js'
import { LiveTelnetSessionController } from '../../dist-electron/main/services/sessions/telnet-session-controller.js'
import { SshTunnelService } from '../../dist-electron/main/services/sessions/ssh-tunnel-service.js'
import { LiveSerialSessionController } from '../../dist-electron/main/services/sessions/serial-session-controller.js'

async function listen(server) {
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  return address.port
}

async function close(server) {
  await new Promise((resolve) => server.close(() => resolve()))
}

async function waitFor(predicate, timeoutMs = 500) {
  const until = Date.now() + timeoutMs
  while (!predicate()) {
    if (Date.now() > until) throw new Error('Timed out waiting for socket activity')
    await new Promise((resolve) => setTimeout(resolve, 2))
  }
}

test('HTTP CONNECT proxy sends authenticated tunnel request and returns a usable socket', async (t) => {
  const requests = []
  const proxy = net.createServer((socket) => {
    socket.once('data', (chunk) => {
      requests.push(chunk.toString('latin1'))
      socket.write('HTTP/1.1 200 Connection Established\r\n\r\n')
    })
  })
  t.after(() => close(proxy))
  const port = await listen(proxy)
  const socket = await createOutboundSocket('target.example.test', 443, {
    type: 'http',
    host: '127.0.0.1',
    port,
    username: 'proxy-user',
    password: 'proxy-password'
  })
  await waitFor(() => requests.length === 1)
  assert.match(requests[0], /^CONNECT target\.example\.test:443 HTTP\/1\.1/m)
  assert.match(requests[0], /Proxy-Authorization: Basic cHJveHktdXNlcjpwcm94eS1wYXNzd29yZA==/)
  socket.destroy()
})

test('Telnet filters IAC negotiation bytes and responds to NAWS', async (t) => {
  const received = []
  const server = net.createServer((socket) => {
    socket.on('data', (chunk) => received.push(chunk))
    socket.write(Buffer.from([255, 253, 31]))
    socket.write('router> ')
  })
  t.after(() => close(server))
  const port = await listen(server)
  const output = []
  const controller = new LiveTelnetSessionController(
    'telnet-test',
    {
      id: 'telnet-profile',
      name: 'Telnet',
      type: 'telnet',
      host: '127.0.0.1',
      port,
      username: '',
      remotePath: '',
      group: '默认'
    },
    (chunk) => output.push(chunk),
    () => {}
  )
  try {
    await controller.connect()
    await controller.resize(100, 40)
    const willNaws = Buffer.from([255, 251, 31])
    const naws = Buffer.from([255, 250, 31, 0, 100, 0, 40, 255, 240])
    await waitFor(() => {
      const wire = Buffer.concat(received)
      return output.join('').includes('router> ') && wire.includes(willNaws) && wire.includes(naws)
    })
    assert.equal(output.join(''), 'router> ')
  } finally {
    await controller.disconnect()
  }
})

test('SSH tunnel service tracks remote tunnel start and cleanup lifecycle', async () => {
  class FakeSsh extends net.Socket {}
  const client = new FakeSsh()
  const calls = []
  client.forwardIn = (host, port, callback) => {
    calls.push(['start', host, port])
    callback()
  }
  client.unforwardIn = (host, port, callback) => {
    calls.push(['stop', host, port])
    callback()
  }
  const service = new SshTunnelService(client, () => {})
  const rule = {
    id: 'remote-1',
    name: 'remote',
    kind: 'remote',
    bindHost: '127.0.0.1',
    bindPort: 15432,
    targetHost: '127.0.0.1',
    targetPort: 5432,
    autoStart: false
  }
  service.register(rule)
  await service.start(rule)
  assert.equal(service.list()[0].status, 'running')
  await service.stop(rule.id)
  assert.equal(service.list()[0].status, 'stopped')
  assert.deepEqual(calls, [
    ['start', '127.0.0.1', 15432],
    ['stop', '127.0.0.1', 15432]
  ])
})

test('SSH tunnel service destroys active local clients before closing its listener', async () => {
  class FakeSsh extends EventEmitter {}
  const client = new FakeSsh()
  client.forwardOut = (_sourceHost, _sourcePort, _targetHost, _targetPort, callback) => {
    const channel = new PassThrough()
    channel.close = () => channel.destroy()
    callback(undefined, channel)
  }
  const service = new SshTunnelService(client, () => {})
  const port = await reservePort()
  const rule = {
    id: 'local-active-client',
    kind: 'local',
    bindHost: '127.0.0.1',
    bindPort: port,
    targetHost: '127.0.0.1',
    targetPort: 5432,
    autoStart: false
  }
  service.register(rule)
  await service.start(rule)
  const socket = net.connect(port, '127.0.0.1')
  await once(socket, 'connect')
  await Promise.race([
    service.stop(rule.id),
    new Promise((_, reject) => setTimeout(() => reject(new Error('tunnel stop timed out')), 500))
  ])
  assert.equal(service.list()[0].status, 'stopped')
  socket.destroy()
})

test('SSH tunnel service keeps a failed remote forward visible for retry', async () => {
  class FakeSsh extends EventEmitter {}
  const client = new FakeSsh()
  let failStop = true
  client.forwardIn = (_host, _port, callback) => callback()
  client.unforwardIn = (_host, _port, callback) =>
    callback(failStop ? new Error('remote denied cancellation') : undefined)
  const service = new SshTunnelService(client, () => {})
  const rule = {
    id: 'remote-retry',
    kind: 'remote',
    bindHost: '127.0.0.1',
    bindPort: 15433,
    targetHost: '127.0.0.1',
    targetPort: 5432,
    autoStart: false
  }
  service.register(rule)
  await service.start(rule)
  await assert.rejects(() => service.stop(rule.id), /remote denied cancellation/)
  assert.equal(service.list()[0].status, 'error')
  failStop = false
  await service.stop(rule.id)
  assert.equal(service.list()[0].status, 'stopped')
})

test('SSH tunnel service rejects duplicate listener endpoints before startup', () => {
  class FakeSsh extends EventEmitter {}
  const service = new SshTunnelService(new FakeSsh(), () => {})
  service.register({
    id: 'local-one',
    kind: 'local',
    bindHost: '127.0.0.1',
    bindPort: 15434,
    targetHost: '127.0.0.1',
    targetPort: 5432,
    autoStart: false
  })
  assert.throws(
    () =>
      service.register({
        id: 'dynamic-conflict',
        kind: 'dynamic',
        bindHost: '127.0.0.1',
        bindPort: 15434,
        autoStart: false
      }),
    /already configured/
  )
})

async function reservePort() {
  const server = net.createServer()
  const port = await listen(server)
  await close(server)
  return port
}

test('Serial controller configures and releases a mock device without a physical port', async () => {
  class FakeSerialPort extends EventEmitter {
    isOpen = false
    writes = []
    open(callback) {
      this.isOpen = true
      callback()
    }
    write(data, callback) {
      this.writes.push(Buffer.from(data))
      callback()
    }
    drain(callback) {
      callback()
    }
    close(callback) {
      this.isOpen = false
      callback()
    }
  }
  const port = new FakeSerialPort()
  const configs = []
  const output = []
  const controller = new LiveSerialSessionController(
    'serial-test',
    {
      id: 'serial-profile',
      name: 'Serial',
      type: 'serial',
      host: '',
      port: 0,
      username: '',
      remotePath: '',
      group: '默认',
      devicePath: '/dev/ttyMOCK0',
      baudRate: 115200,
      dataBits: 8,
      stopBits: 1,
      parity: 'none',
      flowControl: 'software'
    },
    (chunk) => output.push(chunk),
    () => {},
    '',
    (config) => {
      configs.push(config)
      return port
    }
  )
  await controller.connect()
  port.emit('data', Buffer.from('ready\r\n'))
  await controller.write('AT')
  assert.equal(controller.getSummary(), 'Serial /dev/ttyMOCK0 @ 115200')
  assert.equal(output.join(''), 'ready\r\n')
  assert.deepEqual(configs[0], {
    path: '/dev/ttyMOCK0',
    baudRate: 115200,
    dataBits: 8,
    stopBits: 1,
    parity: 'none',
    rtscts: false,
    xon: true,
    xoff: true,
    autoOpen: false
  })
  assert.equal(port.writes[0].toString(), 'AT')
  await controller.disconnect()
  assert.equal(port.isOpen, false)
})
