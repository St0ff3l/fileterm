import { SerialPort } from 'serialport'
import type { SerialProfile, TerminalSessionController } from '@fileterm/core'
import { decodeBuffer, encodeText } from '../text-encoding.js'

export class LiveSerialSessionController implements TerminalSessionController {
  readonly type = 'serial' as const
  private port?: SerialPort
  private connected = false
  private transcript = ''

  constructor(
    readonly id: string,
    private readonly profile: SerialProfile,
    private readonly onData: (chunk: string) => void,
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void,
    initialTranscript = '',
    private readonly createPort: (options: ConstructorParameters<typeof SerialPort>[0]) => SerialPort = (options) =>
      new SerialPort(options)
  ) {
    this.transcript = initialTranscript
  }

  async connect(): Promise<void> {
    const port = this.createPort({
      path: this.profile.devicePath,
      baudRate: this.profile.baudRate,
      dataBits: this.profile.dataBits,
      stopBits: this.profile.stopBits,
      parity: this.profile.parity,
      rtscts: this.profile.flowControl === 'hardware',
      xon: this.profile.flowControl === 'software',
      xoff: this.profile.flowControl === 'software',
      autoOpen: false
    })
    await new Promise<void>((resolve, reject) =>
      port.open((error) => (error ? reject(toSerialError(error, this.profile.devicePath)) : resolve()))
    )
    this.port = port
    this.connected = true
    port.on('data', (chunk: Buffer) => this.append(decodeBuffer(chunk, this.profile.encoding ?? 'UTF-8')))
    port.on('error', (error) => this.close(`Serial error: ${toSerialError(error, this.profile.devicePath).message}`))
    port.on('close', () => this.close('Serial device disconnected'))
    this.onStateChange(this.getSummary(), this.transcript, true)
  }

  async disconnect(): Promise<void> {
    if (!this.connected) return
    const port = this.port
    this.port = undefined
    this.connected = false
    await new Promise<void>((resolve) => {
      if (!port?.isOpen) return resolve()
      port.close(() => resolve())
    })
    this.onStateChange('Serial disconnected', this.transcript, false)
  }

  getSummary() {
    return this.connected ? `Serial ${this.profile.devicePath} @ ${this.profile.baudRate}` : 'Serial disconnected'
  }

  getTerminalTranscript() {
    return this.transcript
  }

  async write(data: string): Promise<void> {
    if (!this.port?.isOpen) throw new Error('Serial session is disconnected')
    const payload = encodeText(data, this.profile.encoding ?? 'UTF-8')
    await new Promise<void>((resolve, reject) =>
      this.port?.write(payload, (error) => (error ? reject(error) : resolve()))
    )
    await new Promise<void>((resolve, reject) => this.port?.drain((error) => (error ? reject(error) : resolve())))
  }

  async resize(): Promise<void> {
    // Serial links have no terminal-size protocol. xterm still resizes locally.
  }

  private append(chunk: string) {
    this.transcript = `${this.transcript}${chunk}`.slice(-200_000)
    this.onData(chunk)
  }

  private close(summary: string) {
    if (!this.connected) return
    this.connected = false
    this.port = undefined
    this.onStateChange(summary, this.transcript, false)
  }
}

function toSerialError(error: Error, devicePath: string) {
  if (/EACCES|permission/i.test(error.message))
    return new Error(`Cannot open ${devicePath}: permission denied. On Linux, add this user to dialout.`)
  if (/ENOENT|not found/i.test(error.message))
    return new Error(`Serial device ${devicePath} is unavailable or was disconnected.`)
  if (/EBUSY|busy/i.test(error.message)) return new Error(`Serial device ${devicePath} is already in use.`)
  return error
}
