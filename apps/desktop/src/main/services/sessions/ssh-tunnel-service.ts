import net, { type Server, type Socket } from 'node:net'
import type { Client, ClientChannel } from 'ssh2'
import type { SshForwardRule, SshTunnelSnapshot } from '@fileterm/core'

/** Owns listeners only for one live SSH tab; nothing here is persisted. */
export class SshTunnelService {
  private readonly listeners = new Map<string, Server>()
  private readonly remoteRules = new Map<string, SshForwardRule>()
  private readonly rules = new Map<string, SshTunnelSnapshot>()
  /** Sockets must be closed before Server.close(), otherwise Node waits forever. */
  private readonly activeSockets = new Map<string, Set<Socket>>()

  constructor(
    private readonly client: Client,
    private readonly onState: (message: string) => void,
    private readonly onChange: (tunnels: SshTunnelSnapshot[]) => void = () => undefined
  ) {
    client.on('tcp connection', (details, accept, reject) => {
      const rule = [...this.remoteRules.values()].find(
        (item) => item.bindPort === details.destPort && matchesRemoteBindHost(item.bindHost, details.destIP)
      )
      if (!rule?.targetHost || !rule.targetPort) return reject()
      const channel = accept()
      if (!channel) return reject()
      const target = net.connect({ host: rule.targetHost, port: rule.targetPort })
      this.trackSocket(rule.id, target)
      target.once('error', () => channel.close())
      target.once('close', () => channel.close())
      channel.once('error', () => target.destroy())
      channel.once('close', () => target.destroy())
      channel.pipe(target).pipe(channel)
    })
  }

  list() {
    return [...this.rules.values()].map((rule) => ({ ...rule }))
  }

  register(rule: SshForwardRule, runtimeOnly = false) {
    validateRule(rule)
    const previous = this.rules.get(rule.id)
    if (previous && (this.listeners.has(rule.id) || this.remoteRules.has(rule.id))) {
      throw new Error(`Tunnel ${rule.id} is already running`)
    }
    this.assertEndpointAvailable(rule)
    this.rules.set(rule.id, { ...rule, status: previous?.status === 'running' ? 'running' : 'stopped', runtimeOnly })
    this.emitChange()
  }

  async start(rule: SshForwardRule): Promise<void> {
    if (this.listeners.has(rule.id) || this.remoteRules.has(rule.id)) return
    this.register(rule, this.rules.get(rule.id)?.runtimeOnly ?? false)
    this.setStatus(rule.id, 'starting')
    try {
      if (rule.kind === 'remote') {
        await new Promise<void>((resolve, reject) =>
          this.client.forwardIn(rule.bindHost, rule.bindPort, (error) => (error ? reject(error) : resolve()))
        )
        this.remoteRules.set(rule.id, rule)
        this.setStatus(rule.id, 'running')
        this.onState(`Remote tunnel listening on ${rule.bindHost}:${rule.bindPort}`)
        return
      }
      const server = net.createServer((socket) => {
        this.trackSocket(rule.id, socket)
        if (rule.kind === 'dynamic') void this.handleDynamicSocket(socket)
        else if (rule.targetHost && rule.targetPort) {
          void this.pipeForward(socket, rule.targetHost, rule.targetPort)
        } else socket.destroy()
      })
      server.on('error', (error) => {
        if (this.listeners.get(rule.id) === server) this.listeners.delete(rule.id)
        this.closeActiveSockets(rule.id)
        this.setStatus(rule.id, 'error', error.message)
      })
      await new Promise<void>((resolve, reject) =>
        server.once('error', reject).listen(rule.bindPort, rule.bindHost, () => {
          server.off('error', reject)
          resolve()
        })
      )
      this.listeners.set(rule.id, server)
      this.setStatus(rule.id, 'running')
      this.onState(
        `${rule.kind === 'dynamic' ? 'SOCKS5' : 'Local'} tunnel listening on ${rule.bindHost}:${rule.bindPort}`
      )
    } catch (error) {
      this.setStatus(rule.id, 'error', error instanceof Error ? error.message : String(error))
      throw error
    }
  }

  async stop(ruleId: string): Promise<void> {
    if (!this.rules.has(ruleId)) throw new Error(`Tunnel ${ruleId} was not found`)
    this.setStatus(ruleId, 'stopping')
    try {
      const listener = this.listeners.get(ruleId)
      if (listener) {
        this.listeners.delete(ruleId)
        this.closeActiveSockets(ruleId)
        await closeServer(listener)
      }
      const remote = this.remoteRules.get(ruleId)
      if (remote) {
        this.closeActiveSockets(ruleId)
        await new Promise<void>((resolve, reject) =>
          this.client.unforwardIn(remote.bindHost, remote.bindPort, (error) => (error ? reject(error) : resolve()))
        )
        this.remoteRules.delete(ruleId)
      }
      this.setStatus(ruleId, 'stopped')
    } catch (error) {
      this.setStatus(ruleId, 'error', error instanceof Error ? error.message : String(error))
      throw error
    }
  }

  async remove(ruleId: string): Promise<void> {
    await this.stop(ruleId)
    this.rules.delete(ruleId)
    this.emitChange()
  }

  async stopAll() {
    await Promise.all(
      [...this.listeners.keys(), ...this.remoteRules.keys()].map((id) => this.stop(id).catch(() => undefined))
    )
  }

  private setStatus(ruleId: string, status: SshTunnelSnapshot['status'], error?: string) {
    const rule = this.rules.get(ruleId)
    if (!rule) return
    const { error: previousError, ...withoutError } = rule
    void previousError
    this.rules.set(ruleId, { ...withoutError, status, ...(error ? { error } : {}) })
    this.emitChange()
  }

  private emitChange() {
    this.onChange(this.list())
  }

  private async pipeForward(socket: Socket, host: string, port: number) {
    try {
      const channel = await new Promise<ClientChannel>((resolve, reject) =>
        this.client.forwardOut(
          socket.localAddress ?? '127.0.0.1',
          socket.localPort ?? 0,
          host,
          port,
          (error, stream) => (error || !stream ? reject(error ?? new Error('SSH forwarding failed')) : resolve(stream))
        )
      )
      socket.pipe(channel).pipe(socket)
      socket.once('error', () => channel.close())
      socket.once('close', () => channel.close())
      channel.once('error', () => socket.destroy())
      channel.once('close', () => socket.destroy())
    } catch (error) {
      this.onState(`Tunnel connection failed: ${error instanceof Error ? error.message : String(error)}`)
      socket.destroy()
    }
  }

  private async handleDynamicSocket(socket: Socket) {
    try {
      const greeting = await readExactly(socket, 2)
      if (greeting[0] !== 5) throw new Error('Only SOCKS5 is supported')
      await readExactly(socket, greeting[1] ?? 0)
      socket.write(Buffer.from([5, 0]))
      const request = await readExactly(socket, 4)
      if (request[0] !== 5 || request[1] !== 1) throw new Error('Only SOCKS CONNECT is supported')
      const host = await readSocksHost(socket, request[3] ?? 0)
      const port = (await readExactly(socket, 2)).readUInt16BE(0)
      const channel = await new Promise<ClientChannel>((resolve, reject) =>
        this.client.forwardOut(
          socket.localAddress ?? '127.0.0.1',
          socket.localPort ?? 0,
          host,
          port,
          (error, stream) => (error || !stream ? reject(error ?? new Error('SSH forwarding failed')) : resolve(stream))
        )
      )
      socket.write(Buffer.from([5, 0, 0, 1, 0, 0, 0, 0, 0, 0]))
      socket.pipe(channel).pipe(socket)
      socket.once('error', () => channel.close())
      socket.once('close', () => channel.close())
      channel.once('error', () => socket.destroy())
      channel.once('close', () => socket.destroy())
    } catch {
      socket.write(Buffer.from([5, 1, 0, 1, 0, 0, 0, 0, 0, 0]))
      socket.destroy()
    }
  }

  private assertEndpointAvailable(rule: SshForwardRule) {
    const conflict = [...this.rules.values()].find(
      (existing) =>
        existing.id !== rule.id &&
        (existing.kind === 'remote') === (rule.kind === 'remote') &&
        existing.bindHost === rule.bindHost &&
        existing.bindPort === rule.bindPort
    )
    if (conflict) {
      throw new Error(`Tunnel ${rule.bindHost}:${rule.bindPort} is already configured (${conflict.id})`)
    }
  }

  private trackSocket(ruleId: string, socket: Socket) {
    const sockets = this.activeSockets.get(ruleId) ?? new Set<Socket>()
    sockets.add(socket)
    this.activeSockets.set(ruleId, sockets)
    socket.once('close', () => {
      sockets.delete(socket)
      if (!sockets.size) this.activeSockets.delete(ruleId)
    })
  }

  private closeActiveSockets(ruleId: string) {
    const sockets = this.activeSockets.get(ruleId)
    if (!sockets) return
    for (const socket of sockets) socket.destroy()
    this.activeSockets.delete(ruleId)
  }
}

function matchesRemoteBindHost(bindHost: string, destinationIp: string) {
  return bindHost === destinationIp || bindHost === '0.0.0.0' || bindHost === '::' || bindHost === '*'
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()))
  })
}

function validateRule(rule: SshForwardRule) {
  if (!rule.id || !Number.isInteger(rule.bindPort) || rule.bindPort < 1 || rule.bindPort > 65535) {
    throw new Error('Tunnel requires a valid bind port')
  }
  if (!rule.bindHost?.trim()) throw new Error('Tunnel requires a bind address')
  if (rule.kind === 'dynamic') return
  const targetPort = rule.targetPort
  if (
    !rule.targetHost?.trim() ||
    targetPort === undefined ||
    !Number.isInteger(targetPort) ||
    targetPort < 1 ||
    targetPort > 65535
  ) {
    throw new Error(`${rule.kind === 'remote' ? 'Remote' : 'Local'} tunnel requires a valid target`)
  }
}

function readExactly(socket: Socket, length: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let data = Buffer.alloc(0)
    const onData = (chunk: Buffer) => {
      data = Buffer.concat([data, chunk])
      if (data.length < length) return
      cleanup()
      if (data.length > length) socket.unshift(data.subarray(length))
      resolve(data.subarray(0, length))
    }
    const fail = (error: Error) => {
      cleanup()
      reject(error)
    }
    const cleanup = () => {
      socket.off('data', onData)
      socket.off('error', fail)
      socket.off('close', closed)
    }
    const closed = () => fail(new Error('SOCKS client disconnected'))
    socket.on('data', onData)
    socket.once('error', fail)
    socket.once('close', closed)
  })
}

async function readSocksHost(socket: Socket, type: number) {
  if (type === 1) return [...(await readExactly(socket, 4))].join('.')
  if (type === 3) return (await readExactly(socket, (await readExactly(socket, 1))[0] ?? 0)).toString('utf8')
  if (type === 4)
    return [...(await readExactly(socket, 16))]
      .reduce(
        (groups: string[], byte, index, bytes) =>
          index % 2
            ? [...groups, `${bytes[index - 1]?.toString(16).padStart(2, '0')}${byte.toString(16).padStart(2, '0')}`]
            : groups,
        []
      )
      .join(':')
  throw new Error('Unsupported SOCKS address type')
}
