import {
  createTerminalOutputDelivery,
  type TerminalOutputDelivery,
  type TerminalOutputListener
} from './events/terminal-output'
import type { BridgeMetadata, BridgeTransport } from './transport'
import { setWebLoginStatus, type WebLoginStatus } from './web-login'

export interface WebBootstrapPayload extends BridgeMetadata {
  serverOs?: string
  webLogin?: WebLoginStatus | null
}

interface ServerWsMessage {
  type: 'event' | 'terminal_data' | 'channel' | 'pong'
  event?: string
  channelId?: number
  payload?: unknown
}

export class WebBridgeTransport implements BridgeTransport {
  readonly clientId: string
  private socket: WebSocket | null = null
  private connectPromise: Promise<void> | null = null
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private pingTimer: ReturnType<typeof setInterval> | null = null
  private reconnectBackoffMs = 1000
  private nextChannelId = 1
  private activeTerminalChannelId: number | null = null
  private terminalSubscriptionRequested = false
  private closed = false

  private readonly eventListeners = new Map<string, Set<(payload: unknown) => void>>()
  private readonly channelListeners = new Map<number, (payload: unknown) => void>()
  private readonly terminalDataListeners = new Set<TerminalOutputListener>()
  private terminalDelivery: ((delivery: TerminalOutputDelivery) => void) | null = null

  private readonly handleBeforeUnload = () => {
    this.dispose()
  }

  constructor(
    private readonly fetchImpl: typeof fetch = fetch.bind(window),
    private readonly createWebSocket: (url: string) => WebSocket = (url) => new WebSocket(url),
    private readonly baseOrigin: string = window.location.origin
  ) {
    this.clientId =
      typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
        ? crypto.randomUUID()
        : `web-${Date.now()}-${Math.random().toString(16).slice(2)}`
    if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
      window.addEventListener('beforeunload', this.handleBeforeUnload)
    }
  }

  async bootstrap(): Promise<WebBootstrapPayload> {
    const response = await this.fetchImpl(`${this.baseOrigin}/api/bootstrap`, {
      credentials: 'same-origin',
      headers: { Accept: 'application/json' }
    })
    if (!response.ok) {
      throw new Error(`Bootstrap failed with HTTP ${response.status}`)
    }
    const metadata = (await response.json()) as WebBootstrapPayload
    setWebLoginStatus(metadata.webLogin ?? null)
    if (metadata.webLogin?.mode !== 'login' && metadata.webLogin?.mode !== 'setup') {
      await this.ensureSocket()
    }
    return metadata
  }

  invoke = async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    return this.sendRpc<T>(command, args)
  }

  invokeWithChannel = async <T, E>(
    command: string,
    args: Record<string, unknown>,
    onEvent: (event: E) => void
  ): Promise<T> => {
    await this.ensureSocket()
    const channelId = this.nextChannelId++
    this.channelListeners.set(channelId, (payload) => {
      const typed = payload as { type?: string }
      onEvent(payload as E)
      if (typed?.type === 'completed' || typed?.type === 'error') {
        this.channelListeners.delete(channelId)
      }
    })
    try {
      return await this.sendRpc<T>(command, args, channelId)
    } catch (error) {
      this.channelListeners.delete(channelId)
      throw error
    }
  }

  subscribe = <T>(eventName: string, listener: (payload: T) => void): (() => void) => {
    let listeners = this.eventListeners.get(eventName)
    if (!listeners) {
      listeners = new Set()
      this.eventListeners.set(eventName, listeners)
    }
    const wrapped = listener as (payload: unknown) => void
    listeners.add(wrapped)
    void this.ensureSocket().catch(() => undefined)
    return () => {
      const current = this.eventListeners.get(eventName)
      if (!current) return
      current.delete(wrapped)
      if (current.size === 0) {
        this.eventListeners.delete(eventName)
      }
    }
  }

  subscribeReady = async <T>(eventName: string, listener: (payload: T) => void): Promise<() => void> => {
    await this.ensureSocket()
    return this.subscribe(eventName, listener)
  }

  subscribeTerminalData = (listener: TerminalOutputListener): (() => void) => {
    this.terminalDataListeners.add(listener)
    this.terminalSubscriptionRequested = true
    void this.ensureSocket()
      .then(() => {
        if (this.terminalSubscriptionRequested && this.activeTerminalChannelId === null) {
          this.sendWs({ type: 'subscribe_terminal' })
        }
      })
      .catch(() => undefined)
    return () => {
      this.terminalDataListeners.delete(listener)
    }
  }

  dispose() {
    this.closed = true
    if (typeof window !== 'undefined' && typeof window.removeEventListener === 'function') {
      window.removeEventListener('beforeunload', this.handleBeforeUnload)
    }
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer)
      this.pingTimer = null
    }
    if (this.socket) {
      this.socket.close()
      this.socket = null
    }
  }

  private async sendRpc<T>(command: string, args: Record<string, unknown>, channelId?: number): Promise<T> {
    const response = await this.fetchImpl(`${this.baseOrigin}/api/rpc`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json'
      },
      body: JSON.stringify({
        command,
        args,
        clientId: this.clientId,
        windowLabel: 'main',
        channelId
      })
    })
    const body = (await response.json().catch(() => null)) as { data?: T; error?: string } | null
    if (!response.ok) {
      throw new Error(body?.error || `RPC ${command} failed with HTTP ${response.status}`)
    }
    return (body?.data ?? null) as T
  }

  private ensureSocket(): Promise<void> {
    if (this.closed) {
      return Promise.reject(new Error('Web bridge disposed'))
    }
    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      return Promise.resolve()
    }
    if (this.connectPromise) {
      return this.connectPromise
    }

    const wsOrigin = this.baseOrigin.replace(/^http/, 'ws')
    const url = `${wsOrigin}/api/ws?clientId=${encodeURIComponent(this.clientId)}&windowLabel=main`

    this.connectPromise = new Promise<void>((resolve, reject) => {
      const ws = this.createWebSocket(url)
      this.socket = ws
      let settled = false

      ws.addEventListener('open', () => {
        settled = true
        this.connectPromise = null
        this.reconnectBackoffMs = 1000
        this.activeTerminalChannelId = null
        this.startHeartbeat()
        if (this.terminalSubscriptionRequested) {
          this.sendWs({ type: 'subscribe_terminal' })
        }
        resolve()
      })

      ws.addEventListener('message', (event) => {
        if (typeof event.data !== 'string') return
        let message: ServerWsMessage
        try {
          message = JSON.parse(event.data) as ServerWsMessage
        } catch {
          return
        }
        this.handleServerMessage(message)
      })

      ws.addEventListener('error', () => {
        if (!settled) {
          settled = true
          this.connectPromise = null
          reject(new Error('WebSocket connection failed'))
        }
      })

      ws.addEventListener('close', () => {
        if (!settled) {
          settled = true
          this.connectPromise = null
          reject(new Error('WebSocket closed before opening'))
        }
        if (this.socket === ws) {
          this.socket = null
          this.activeTerminalChannelId = null
        }
        if (this.pingTimer !== null) {
          clearInterval(this.pingTimer)
          this.pingTimer = null
        }
        if (!this.closed) {
          this.scheduleReconnect()
        }
      })
    })

    return this.connectPromise
  }

  private ensureTerminalDelivery(channelId: number) {
    if (this.activeTerminalChannelId === channelId && this.terminalDelivery) {
      return
    }
    this.activeTerminalChannelId = channelId
    this.terminalDelivery = createTerminalOutputDelivery(
      this.terminalDataListeners,
      async (deliveryId) => {
        this.sendWs({
          type: 'ack_terminal',
          channelId,
          deliveryId
        })
      },
      (diagnostic) => {
        void this.invoke('app_write_diagnostic_log', {
          level: 'WARN',
          scope: 'renderer:performance',
          message: diagnostic
        }).catch(() => undefined)
      }
    )
  }

  private handleServerMessage(message: ServerWsMessage) {
    if (message.type === 'pong') {
      return
    }
    if (message.type === 'event' && message.event) {
      if (message.event === 'web:terminal-subscribed') {
        const channelId = (message.payload as { channelId?: number } | undefined)?.channelId
        if (typeof channelId === 'number') {
          this.ensureTerminalDelivery(channelId)
        }
        return
      }
      if (message.event === 'web:events-lagged') {
        void this.invoke('app_get_snapshot')
          .then((snapshot) => this.emitEvent('workspace:snapshot', snapshot))
          .catch(() => undefined)
        return
      }
      this.emitEvent(message.event, message.payload)
      return
    }
    if (message.type === 'terminal_data' && message.payload) {
      if (typeof message.channelId === 'number') {
        this.ensureTerminalDelivery(message.channelId)
      }
      this.terminalDelivery?.(message.payload as TerminalOutputDelivery)
      return
    }
    if (message.type === 'channel' && typeof message.channelId === 'number') {
      this.channelListeners.get(message.channelId)?.(message.payload)
    }
  }

  private emitEvent(eventName: string, payload: unknown) {
    const listeners = this.eventListeners.get(eventName)
    if (!listeners) return
    for (const listener of [...listeners]) {
      try {
        listener(payload)
      } catch (error) {
        console.error(`Error in web event listener for ${eventName}:`, error)
      }
    }
  }

  private sendWs(payload: Record<string, unknown>) {
    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(payload))
    }
  }

  private startHeartbeat() {
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer)
    }
    this.pingTimer = setInterval(() => {
      this.sendWs({ type: 'ping' })
    }, 25_000)
  }

  private scheduleReconnect() {
    if (this.reconnectTimer !== null || this.closed) return
    const delay = this.reconnectBackoffMs
    this.reconnectBackoffMs = Math.min(this.reconnectBackoffMs * 2, 15_000)
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      void this.ensureSocket()
        .then(() => this.invoke('app_get_snapshot'))
        .then((snapshot) => this.emitEvent('workspace:snapshot', snapshot))
        .catch(() => undefined)
    }, delay)
  }
}
