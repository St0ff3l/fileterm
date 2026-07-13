import type { Socket } from 'node:net'
import type { TelnetProfile, TerminalSessionController } from '@fileterm/core'
import { createOutboundSocket } from '../network/proxy-socket-factory.js'
import { decodeBuffer, encodeText } from '../text-encoding.js'

const IAC = 255
const DONT = 254
const DO = 253
const WONT = 252
const WILL = 251
const SB = 250
const SE = 240
const TERMINAL_TYPE = 24
const NAWS = 31

export class LiveTelnetSessionController implements TerminalSessionController {
  readonly type = 'telnet' as const
  private socket?: Socket
  private connected = false
  private transcript = ''
  private cols = 120
  private rows = 32
  private parseState: 'data' | 'iac' | 'option' | 'subnegotiation' | 'subnegotiation-iac' = 'data'
  private pendingCommand = 0
  private subnegotiation: number[] = []

  constructor(
    readonly id: string,
    private readonly profile: TelnetProfile,
    private readonly onData: (chunk: string) => void,
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void,
    initialTranscript = ''
  ) {
    this.transcript = initialTranscript
  }

  async connect(): Promise<void> {
    const socket = await createOutboundSocket(this.profile.host, this.profile.port, this.profile.proxy)
    this.socket = socket
    this.connected = true
    socket.on('data', (chunk: Buffer) => this.handleData(chunk))
    socket.on('error', (error) => this.close(`Telnet error: ${error.message}`))
    socket.on('close', () => this.close('Telnet disconnected'))
    this.onStateChange(this.getSummary(), this.transcript, true)
  }

  async disconnect(): Promise<void> {
    this.close('Telnet disconnected')
  }

  getSummary() {
    return this.connected ? `Telnet ${this.profile.host}:${this.profile.port}` : 'Telnet disconnected'
  }

  getTerminalTranscript() {
    return this.transcript
  }

  async write(data: string): Promise<void> {
    if (!this.socket || !this.connected) throw new Error('Telnet session is disconnected')
    const payload = Buffer.from(
      [...encodeText(data, this.profile.encoding ?? 'UTF-8')].flatMap((byte) => (byte === IAC ? [IAC, IAC] : [byte]))
    )
    await new Promise<void>((resolve, reject) =>
      this.socket?.write(payload, (error) => (error ? reject(error) : resolve()))
    )
  }

  async resize(cols: number, rows: number): Promise<void> {
    this.cols = cols
    this.rows = rows
    this.sendNaws()
  }

  private handleData(chunk: Buffer) {
    const output: number[] = []
    for (const byte of chunk) {
      if (this.parseState === 'data') {
        if (byte === IAC) this.parseState = 'iac'
        else output.push(byte)
        continue
      }
      if (this.parseState === 'iac') {
        if (byte === IAC) {
          output.push(IAC)
          this.parseState = 'data'
        } else if (byte === DO || byte === DONT || byte === WILL || byte === WONT) {
          this.pendingCommand = byte
          this.parseState = 'option'
        } else if (byte === SB) {
          this.subnegotiation = []
          this.parseState = 'subnegotiation'
        } else this.parseState = 'data'
        continue
      }
      if (this.parseState === 'option') {
        this.respondOption(this.pendingCommand, byte)
        this.parseState = 'data'
        continue
      }
      if (this.parseState === 'subnegotiation') {
        if (byte === IAC) this.parseState = 'subnegotiation-iac'
        else this.subnegotiation.push(byte)
        continue
      }
      if (byte === SE) this.handleSubnegotiation(this.subnegotiation)
      else if (byte === IAC) this.subnegotiation.push(IAC)
      this.parseState = 'subnegotiation'
    }
    if (output.length) this.append(decodeBuffer(Buffer.from(output), this.profile.encoding ?? 'UTF-8'))
  }

  private respondOption(command: number, option: number) {
    const supported = option === 1 || option === 3 || option === 0 || option === TERMINAL_TYPE || option === NAWS
    if (command === DO) {
      this.sendCommand(supported ? WILL : WONT, option)
      if (option === NAWS) this.sendNaws()
    } else if (command === WILL) this.sendCommand(supported ? DO : DONT, option)
    else if (command === DONT) this.sendCommand(WONT, option)
    else this.sendCommand(DONT, option)
  }

  private handleSubnegotiation(values: number[]) {
    if (values[0] === TERMINAL_TYPE && values[1] === 1) {
      this.socket?.write(Buffer.from([IAC, SB, TERMINAL_TYPE, 0, ...Buffer.from('xterm-256color'), IAC, SE]))
    }
  }

  private sendNaws() {
    if (!this.socket || !this.connected) return
    this.socket.write(
      Buffer.from([IAC, SB, NAWS, this.cols >> 8, this.cols & 0xff, this.rows >> 8, this.rows & 0xff, IAC, SE])
    )
  }

  private sendCommand(command: number, option: number) {
    this.socket?.write(Buffer.from([IAC, command, option]))
  }

  private append(chunk: string) {
    this.transcript = `${this.transcript}${chunk}`.slice(-200_000)
    this.onData(chunk)
  }

  private close(summary: string) {
    if (!this.connected) return
    this.connected = false
    const socket = this.socket
    this.socket = undefined
    socket?.removeAllListeners()
    socket?.destroy()
    this.onStateChange(summary, this.transcript, false)
  }
}
