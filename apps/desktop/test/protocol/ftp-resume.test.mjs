import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createServer as createNetServer } from 'node:net'
import { mkdtemp, mkdir, open, readFile, rename, rm, stat, unlink, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { createSecureContext, createServer as createTlsServer, TLSSocket } from 'node:tls'
import test from 'node:test'
import {
  LiveFtpSessionController,
  resolveFtpSecureOption
} from '../../dist-electron/main/services/sessions/ftp-session-controller.js'

test('FTP resumes upload/download and safely renames the completed checkpoint', async (t) => {
  const fixture = await createFixture(t, { tlsMode: 'none' })
  if (!fixture) return
  const controller = createController(fixture, 'none')
  t.after(() => controller.disconnect())
  await controller.connect()

  const source = path.join(fixture.localDir, 'source.txt')
  const download = path.join(fixture.localDir, 'download.txt')
  await writeFile(source, 'hello resumable world')
  await fixture.server.writeRemote('/target.txt.fileterm-part', 'hello ')

  await controller.uploadFile(source, '/target.txt.fileterm-part', () => undefined, { resumeOffset: 6 })
  assert.equal(await fixture.server.readRemote('/target.txt.fileterm-part'), 'hello resumable world')
  await controller.replaceRemoteFile('/target.txt.fileterm-part', '/target.txt')
  assert.equal(await fixture.server.readRemote('/target.txt'), 'hello resumable world')

  await writeFile(download, 'hello ')
  await controller.downloadFile('/target.txt', download, () => undefined, { resumeOffset: 6 })
  assert.equal(await readFile(download, 'utf8'), 'hello resumable world')

  const emptySource = path.join(fixture.localDir, 'empty.txt')
  const emptyDownload = path.join(fixture.localDir, 'empty-download.txt')
  await writeFile(emptySource, '')
  await controller.uploadFile(emptySource, '/empty.part', () => undefined)
  assert.equal(await fixture.server.readRemote('/empty.part'), '')
  await controller.downloadFile('/empty.part', emptyDownload, () => undefined)
  assert.equal((await stat(emptyDownload)).size, 0)
  await assert.rejects(
    () => controller.uploadFile(emptySource, '/empty.part', () => undefined, { resumeOffset: 1 }),
    /断点大于源文件/
  )
})

test('FTP falls back from unsupported APPE to REST + STOR', async (t) => {
  const fixture = await createFixture(t, { tlsMode: 'none', rejectAppe: true })
  if (!fixture) return
  const controller = createController(fixture, 'none')
  t.after(() => controller.disconnect())
  await controller.connect()

  const source = path.join(fixture.localDir, 'rest-source.txt')
  await writeFile(source, 'prefix-and-remainder')
  await fixture.server.writeRemote('/rest.part', 'prefix-')
  await controller.uploadFile(source, '/rest.part', () => undefined, { resumeOffset: 7 })
  assert.equal(await fixture.server.readRemote('/rest.part'), 'prefix-and-remainder')
})

test('FTP security modes map to plain, explicit and implicit basic-ftp options', async () => {
  assert.equal(resolveFtpSecureOption({ secure: false }), false)
  assert.equal(resolveFtpSecureOption({ secure: true }), true)
  assert.equal(resolveFtpSecureOption({ secure: true, securityMode: 'implicit' }), 'implicit')
})

test('FTP controller executes APPE -> REST/STOR fallback without a network server', async (t) => {
  const localDir = await mkdtemp(path.join(os.tmpdir(), 'fileterm-ftp-fake-'))
  t.after(() => rm(localDir, { recursive: true, force: true }))
  const source = path.join(localDir, 'source.txt')
  await writeFile(source, 'prefix-and-remainder')
  const fake = new FakeBasicFtpClient(new Map([['/target.part', Buffer.from('prefix-')]]))
  const controller = new LiveFtpSessionController(
    'fake',
    {
      id: 'fake-profile',
      type: 'ftp',
      name: 'Fake FTP',
      host: 'fake',
      port: 21,
      username: 'test',
      secure: false,
      securityMode: 'none',
      group: 'test',
      remotePath: '/'
    },
    fake
  )
  await controller.connect()
  await controller.uploadFile(source, '/target.part', () => undefined, { resumeOffset: 7 })
  assert.equal(fake.files.get('/target.part').toString(), 'prefix-and-remainder')
  assert.deepEqual(fake.commands, ['REST 7'])

  fake.closed = true
  await controller.uploadFile(source, '/reconnected.part', () => undefined)
  assert.equal(fake.accessCalls, 2)
})

for (const tlsMode of ['explicit', 'implicit']) {
  test(`${tlsMode} FTPS establishes a protected control/data transfer`, async (t) => {
    const fixture = await createFixture(t, { tlsMode })
    if (!fixture) return
    const controller = createController(fixture, tlsMode)
    t.after(() => controller.disconnect())
    await controller.connect()

    const source = path.join(fixture.localDir, `${tlsMode}.txt`)
    await writeFile(source, `${tlsMode}-ftps`)
    await controller.uploadFile(source, `/${tlsMode}.part`, () => undefined)
    assert.equal(await fixture.server.readRemote(`/${tlsMode}.part`), `${tlsMode}-ftps`)
  })
}

function createController(fixture, securityMode) {
  return new LiveFtpSessionController(
    `ftp-${securityMode}`,
    {
      id: `profile-${securityMode}`,
      type: 'ftp',
      name: `FTP ${securityMode}`,
      host: '127.0.0.1',
      port: fixture.server.port,
      username: 'test',
      password: 'test',
      secure: securityMode !== 'none',
      securityMode,
      group: 'test',
      remotePath: '/'
    },
    undefined,
    securityMode === 'none' ? undefined : { rejectUnauthorized: false }
  )
}

async function createFixture(t, options) {
  const localDir = await mkdtemp(path.join(os.tmpdir(), 'fileterm-ftp-local-'))
  const remoteDir = await mkdtemp(path.join(os.tmpdir(), 'fileterm-ftp-remote-'))
  let tls
  if (options.tlsMode !== 'none') {
    try {
      tls = await createCertificate(localDir)
    } catch (error) {
      if (error?.code === 'ENOENT' || error?.status) {
        t.skip('系统未提供可用的 openssl，跳过本地 FTPS 证书夹具')
        await rm(localDir, { recursive: true, force: true })
        await rm(remoteDir, { recursive: true, force: true })
        return null
      }
      throw error
    }
  }
  const server = new MiniFtpServer(remoteDir, { ...options, tls })
  try {
    await server.start()
  } catch (error) {
    if (error?.code === 'EPERM') {
      t.skip('当前沙箱禁止监听 localhost；协议测试保留给非沙箱 CI/本机执行')
      await rm(localDir, { recursive: true, force: true })
      await rm(remoteDir, { recursive: true, force: true })
      return null
    }
    throw error
  }
  t.after(async () => {
    await server.stop()
    await rm(localDir, { recursive: true, force: true })
    await rm(remoteDir, { recursive: true, force: true })
  })
  return { localDir, remoteDir, server }
}

async function createCertificate(directory) {
  const keyPath = path.join(directory, 'key.pem')
  const certPath = path.join(directory, 'cert.pem')
  execFileSync(
    'openssl',
    [
      'req',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-nodes',
      '-keyout',
      keyPath,
      '-out',
      certPath,
      '-days',
      '1',
      '-subj',
      '/CN=localhost',
      '-addext',
      'subjectAltName=DNS:localhost,IP:127.0.0.1'
    ],
    { stdio: 'ignore' }
  )
  return {
    key: await readFile(keyPath),
    cert: await readFile(certPath)
  }
}

class MiniFtpServer {
  constructor(root, options) {
    this.root = root
    this.options = options
    this.server = undefined
    this.sessions = new Set()
    this.sockets = new Set()
  }

  async start() {
    const listener = (socket) => {
      this.trackSocket(socket)
      const session = new MiniFtpSession(this, socket)
      this.sessions.add(session)
      session.start()
    }
    this.server =
      this.options.tlsMode === 'implicit' ? createTlsServer(this.options.tls, listener) : createNetServer(listener)
    await new Promise((resolve, reject) => {
      this.server.once('error', reject)
      this.server.listen(0, '127.0.0.1', resolve)
    })
    this.port = this.server.address().port
  }

  async stop() {
    for (const session of this.sessions) {
      session.close()
    }
    for (const socket of this.sockets) {
      socket.destroy()
    }
    this.server.closeAllConnections?.()
    await new Promise((resolve) => {
      let settled = false
      const finish = () => {
        if (settled) return
        settled = true
        resolve()
      }
      this.server.close(finish)
      setTimeout(finish, 500).unref()
    })
  }

  trackSocket(socket) {
    this.sockets.add(socket)
    socket.once('close', () => this.sockets.delete(socket))
  }

  resolve(remotePath) {
    const normalized = path.posix.normalize(`/${remotePath}`).replace(/^\/+/, '')
    const resolved = path.join(this.root, ...normalized.split('/'))
    if (resolved !== this.root && !resolved.startsWith(`${this.root}${path.sep}`)) {
      throw new Error('path traversal')
    }
    return resolved
  }

  async readRemote(remotePath) {
    return readFile(this.resolve(remotePath), 'utf8')
  }

  async writeRemote(remotePath, content) {
    const target = this.resolve(remotePath)
    await mkdir(path.dirname(target), { recursive: true })
    await writeFile(target, content)
  }
}

class MiniFtpSession {
  constructor(server, socket) {
    this.server = server
    this.socket = socket
    this.buffer = ''
    this.cwd = '/'
    this.restOffset = 0
    this.renameFrom = undefined
    this.dataServer = undefined
    this.dataSocketPromise = undefined
    this.controlProtected = server.options.tlsMode === 'implicit'
    this.dataProtected = this.controlProtected
  }

  start() {
    this.attach(this.socket)
    this.reply(220, 'FileTerm test FTP ready')
  }

  attach(socket) {
    socket.setEncoding('utf8')
    socket.on('data', (chunk) => {
      this.buffer += chunk
      let separator
      while ((separator = this.buffer.indexOf('\r\n')) >= 0) {
        const line = this.buffer.slice(0, separator)
        this.buffer = this.buffer.slice(separator + 2)
        void this.handle(line)
      }
    })
    socket.on('error', () => undefined)
  }

  close() {
    this.dataServer?.close()
    this.socket.destroy()
  }

  reply(code, message) {
    this.socket.write(`${code} ${message}\r\n`)
  }

  async handle(line) {
    const space = line.indexOf(' ')
    const command = (space < 0 ? line : line.slice(0, space)).toUpperCase()
    const argument = space < 0 ? '' : line.slice(space + 1)

    if (command === 'AUTH' && this.server.options.tlsMode === 'explicit') {
      this.reply(234, 'AUTH TLS accepted')
      const plainSocket = this.socket
      plainSocket.removeAllListeners('data')
      this.buffer = ''
      const secureSocket = new TLSSocket(plainSocket, {
        isServer: true,
        secureContext: createSecureContext(this.server.options.tls)
      })
      this.socket = secureSocket
      this.server.trackSocket(secureSocket)
      this.controlProtected = true
      this.attach(secureSocket)
      return
    }
    if (command === 'USER') return this.reply(331, 'Password required')
    if (command === 'PASS') return this.reply(230, 'Logged in')
    if (command === 'TYPE' || command === 'STRU' || command === 'OPTS' || command === 'PBSZ')
      return this.reply(200, 'OK')
    if (command === 'PROT') {
      this.dataProtected = argument.toUpperCase() === 'P'
      return this.reply(200, 'Protection set')
    }
    if (command === 'FEAT') {
      this.socket.write('211-Features\r\n EPSV\r\n SIZE\r\n MDTM\r\n REST STREAM\r\n211 End\r\n')
      return
    }
    if (command === 'PWD') return this.reply(257, `"${this.cwd}" is current directory`)
    if (command === 'CWD') {
      this.cwd = this.remotePath(argument)
      return this.reply(250, 'Directory changed')
    }
    if (command === 'MKD') {
      await mkdir(this.localPath(argument), { recursive: true })
      return this.reply(257, 'Directory created')
    }
    if (command === 'EPSV' || command === 'PASV') return this.openDataServer(command)
    if (command === 'SIZE') {
      try {
        return this.reply(213, String((await stat(this.localPath(argument))).size))
      } catch {
        return this.reply(550, 'File not found')
      }
    }
    if (command === 'MDTM') {
      try {
        const info = await stat(this.localPath(argument))
        return this.reply(213, formatFtpDate(info.mtime))
      } catch {
        return this.reply(550, 'File not found')
      }
    }
    if (command === 'REST') {
      this.restOffset = Number(argument) || 0
      return this.reply(350, `Restarting at ${this.restOffset}`)
    }
    if (command === 'APPE' && this.server.options.rejectAppe) {
      this.discardDataConnection()
      return this.reply(502, 'APPE not implemented')
    }
    if (command === 'STOR' || command === 'APPE') return this.receiveFile(command, argument)
    if (command === 'RETR') return this.sendFile(argument)
    if (command === 'RNFR') {
      this.renameFrom = this.localPath(argument)
      return this.reply(350, 'Ready for RNTO')
    }
    if (command === 'RNTO') {
      await rename(this.renameFrom, this.localPath(argument))
      this.renameFrom = undefined
      return this.reply(250, 'Renamed')
    }
    if (command === 'DELE') {
      await unlink(this.localPath(argument)).catch(() => undefined)
      return this.reply(250, 'Deleted')
    }
    if (command === 'QUIT') {
      this.reply(221, 'Bye')
      return this.socket.end()
    }
    this.discardDataConnection()
    this.reply(502, `${command} not implemented`)
  }

  remotePath(value) {
    return value.startsWith('/') ? path.posix.normalize(value) : path.posix.join(this.cwd, value)
  }

  localPath(value) {
    return this.server.resolve(this.remotePath(value))
  }

  async openDataServer(command) {
    const listener = (socket) => {
      this.server.trackSocket(socket)
      this.dataSocketResolve(socket)
    }
    this.dataServer = this.dataProtected
      ? createTlsServer(this.server.options.tls, listener)
      : createNetServer(listener)
    this.dataSocketPromise = new Promise((resolve) => {
      this.dataSocketResolve = resolve
    })
    await new Promise((resolve, reject) => {
      this.dataServer.once('error', reject)
      this.dataServer.listen(0, '127.0.0.1', resolve)
    })
    const port = this.dataServer.address().port
    if (command === 'EPSV') {
      this.socket.write(`229 Entering Extended Passive Mode (|||${port}|)\r\n`)
    } else {
      this.socket.write(`227 Entering Passive Mode (127,0,0,1,${Math.floor(port / 256)},${port % 256})\r\n`)
    }
  }

  async takeDataSocket() {
    const socket = await this.dataSocketPromise
    this.dataServer.close()
    this.dataServer = undefined
    this.dataSocketPromise = undefined
    return socket
  }

  discardDataConnection() {
    const socketPromise = this.dataSocketPromise
    this.dataServer?.closeAllConnections?.()
    this.dataServer?.close()
    this.dataServer = undefined
    this.dataSocketPromise = undefined
    void socketPromise?.then((socket) => socket.destroy())
  }

  async receiveFile(command, remotePath) {
    this.reply(150, 'Opening data connection')
    const socket = await this.takeDataSocket()
    const chunks = []
    for await (const chunk of socket) chunks.push(chunk)
    const payload = Buffer.concat(chunks)
    const target = this.localPath(remotePath)
    await mkdir(path.dirname(target), { recursive: true })
    if (command === 'APPE') {
      const handle = await open(target, 'a')
      await handle.write(payload)
      await handle.close()
    } else if (this.restOffset > 0) {
      const handle = await open(target, 'r+')
      await handle.write(payload, 0, payload.length, this.restOffset)
      await handle.truncate(this.restOffset + payload.length)
      await handle.close()
    } else {
      await writeFile(target, payload)
    }
    this.restOffset = 0
    this.reply(226, 'Transfer complete')
  }

  async sendFile(remotePath) {
    this.reply(150, 'Opening data connection')
    const socket = await this.takeDataSocket()
    const payload = await readFile(this.localPath(remotePath))
    socket.end(payload.subarray(this.restOffset))
    await new Promise((resolve) => socket.once('close', resolve))
    this.restOffset = 0
    this.reply(226, 'Transfer complete')
  }
}

function formatFtpDate(date) {
  return date.toISOString().replace(/[-:T]/g, '').slice(0, 14)
}

class FakeBasicFtpClient {
  constructor(files) {
    this.files = files
    this.commands = []
    this.accessCalls = 0
    this.closed = false
    this.progress = undefined
    this.parseList = () => []
  }

  async access(options) {
    this.accessCalls += 1
    this.accessOptions = options
    this.closed = false
  }
  async cd() {}
  async pwd() {
    return '/'
  }
  close() {
    this.closed = true
  }
  trackProgress(handler) {
    this.progress = handler
  }
  async ensureDir() {}
  async size(remotePath) {
    const value = this.files.get(remotePath)
    if (!value) throw new Error('550 File not found')
    return value.length
  }
  async lastMod() {
    return new Date(0)
  }
  async appendFrom() {
    throw new Error('502 APPE not implemented')
  }
  async send(command) {
    this.commands.push(command)
    this.restOffset = Number(command.split(' ')[1]) || 0
    return { code: 350, message: 'Restart accepted' }
  }
  async uploadFrom(localPath, remotePath, options = {}) {
    const source = await readFile(localPath)
    const localStart = options.localStart ?? 0
    const prefix = this.files.get(remotePath) ?? Buffer.alloc(0)
    const restart = this.restOffset ?? 0
    this.files.set(remotePath, Buffer.concat([prefix.subarray(0, restart), source.subarray(localStart)]))
    this.progress?.({
      bytes: source.length - localStart,
      bytesOverall: source.length - localStart,
      name: remotePath,
      type: 'upload'
    })
    this.restOffset = 0
    return { code: 226, message: 'OK' }
  }
  async remove(remotePath) {
    this.files.delete(remotePath)
  }
}
