import net, { type Socket } from 'node:net'
import type { ProxyConfig } from '@fileterm/core'

const DEFAULT_TIMEOUT_MS = 15_000

export class ProxyConnectionError extends Error {
  constructor(
    message: string,
    readonly stage: 'proxy' | 'target'
  ) {
    super(message)
    this.name = 'ProxyConnectionError'
  }
}

export async function createOutboundSocket(
  host: string,
  port: number,
  proxy?: ProxyConfig,
  timeoutMs = DEFAULT_TIMEOUT_MS
): Promise<Socket> {
  if (!proxy || proxy.type === 'none') return connectTcp(host, port, timeoutMs, 'target')
  const socket = await connectTcp(proxy.host, proxy.port, timeoutMs, 'proxy')
  try {
    if (proxy.type === 'http') await establishHttpConnect(socket, host, port, proxy, timeoutMs)
    else await establishSocks5Connect(socket, host, port, proxy, timeoutMs)
    socket.setTimeout(0)
    return socket
  } catch (error) {
    socket.destroy()
    throw error
  }
}

function connectTcp(host: string, port: number, timeoutMs: number, stage: 'proxy' | 'target'): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host, port })
    const fail = (error: Error) => {
      socket.destroy()
      reject(
        new ProxyConnectionError(`${stage === 'proxy' ? 'Proxy' : 'Target'} connection failed: ${error.message}`, stage)
      )
    }
    socket.once('error', fail)
    socket.setTimeout(timeoutMs, () => fail(new Error('Connection timed out')))
    socket.once('connect', () => {
      socket.off('error', fail)
      resolve(socket)
    })
  })
}

function readUntil(socket: Socket, predicate: (data: Buffer) => boolean, timeoutMs: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let collected = Buffer.alloc(0)
    const cleanup = () => {
      socket.off('data', onData)
      socket.off('error', onError)
      socket.off('close', onClose)
      clearTimeout(timer)
    }
    const done = (error?: Error) => {
      cleanup()
      if (error) reject(error)
      else resolve(collected)
    }
    const onData = (chunk: Buffer) => {
      collected = Buffer.concat([collected, chunk])
      if (collected.length > 64 * 1024) return done(new Error('Proxy response is too large'))
      if (predicate(collected)) done()
    }
    const onError = (error: Error) => done(error)
    const onClose = () => done(new Error('Proxy closed connection'))
    const timer = setTimeout(() => done(new Error('Proxy handshake timed out')), timeoutMs)
    socket.on('data', onData)
    socket.once('error', onError)
    socket.once('close', onClose)
  })
}

async function establishHttpConnect(socket: Socket, host: string, port: number, proxy: ProxyConfig, timeoutMs: number) {
  const authority = host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`
  const auth = proxy.username
    ? `Proxy-Authorization: Basic ${Buffer.from(`${proxy.username}:${proxy.password ?? ''}`).toString('base64')}\r\n`
    : ''
  socket.write(`CONNECT ${authority} HTTP/1.1\r\nHost: ${authority}\r\n${auth}\r\n`)
  const response = await readUntil(socket, (data) => data.includes(Buffer.from('\r\n\r\n')), timeoutMs)
  const status = response.toString('latin1').match(/^HTTP\/\d\.\d\s+(\d+)/i)?.[1]
  if (status !== '200') throw new ProxyConnectionError(`HTTP CONNECT failed (${status ?? 'invalid response'})`, 'proxy')
}

async function establishSocks5Connect(
  socket: Socket,
  host: string,
  port: number,
  proxy: ProxyConfig,
  timeoutMs: number
) {
  const methods = proxy.username ? Buffer.from([0x05, 0x02, 0x00, 0x02]) : Buffer.from([0x05, 0x01, 0x00])
  socket.write(methods)
  const greeting = await readUntil(socket, (data) => data.length >= 2, timeoutMs)
  if (greeting[0] !== 0x05 || greeting[1] === 0xff)
    throw new ProxyConnectionError('SOCKS5 authentication was rejected', 'proxy')
  if (greeting[1] === 0x02) {
    const username = Buffer.from(proxy.username ?? '')
    const password = Buffer.from(proxy.password ?? '')
    if (!username.length || username.length > 255 || password.length > 255)
      throw new ProxyConnectionError('Invalid SOCKS5 credentials', 'proxy')
    socket.write(
      Buffer.concat([Buffer.from([0x01, username.length]), username, Buffer.from([password.length]), password])
    )
    const auth = await readUntil(socket, (data) => data.length >= 2, timeoutMs)
    if (auth[1] !== 0x00) throw new ProxyConnectionError('SOCKS5 authentication failed', 'proxy')
  }
  const address = encodeSocksAddress(host)
  socket.write(Buffer.concat([Buffer.from([0x05, 0x01, 0x00]), address, Buffer.from([port >> 8, port & 0xff])]))
  const response = await readUntil(
    socket,
    (data) => data.length >= 5 && data.length >= socksResponseLength(data),
    timeoutMs
  )
  if (response[1] !== 0x00) throw new ProxyConnectionError(`SOCKS5 CONNECT failed (code ${response[1]})`, 'proxy')
}

function encodeSocksAddress(host: string): Buffer {
  if (net.isIPv4(host)) return Buffer.from([0x01, ...host.split('.').map(Number)])
  if (net.isIPv6(host)) {
    const groups = host.split(':')
    const expanded = expandIpv6(groups)
    return Buffer.from([
      0x04,
      ...expanded.flatMap((group) => [Number.parseInt(group.slice(0, 2), 16), Number.parseInt(group.slice(2), 16)])
    ])
  }
  const name = Buffer.from(host)
  if (!name.length || name.length > 255) throw new ProxyConnectionError('Invalid proxy destination host', 'target')
  return Buffer.concat([Buffer.from([0x03, name.length]), name])
}

function expandIpv6(groups: string[]) {
  const empty = groups.indexOf('')
  const explicit = groups.filter(Boolean)
  const filled =
    empty === -1
      ? explicit
      : [...explicit.slice(0, empty), ...Array(8 - explicit.length).fill('0'), ...explicit.slice(empty)]
  return filled.map((group) => group.padStart(4, '0'))
}

function socksResponseLength(data: Buffer) {
  if (data[3] === 0x01) return 10
  if (data[3] === 0x04) return 22
  return data.length >= 5 ? 7 + data[4] : Number.MAX_SAFE_INTEGER
}
