// Resilient room connection: authenticates with the session token, keeps the
// server clock estimate fresh, and reconnects with exponential backoff.

import { ServerClock } from './clock'
import { CloseCode, type ClientEvent, type ServerEvent } from './protocol'

export type SocketStatus = 'connecting' | 'open' | 'reconnecting' | 'closed'
export type FatalReason = 'unauthorized' | 'not_found' | 'room_full' | 'left' | 'kicked' | 'too_many_tabs'

export interface SocketHandlers {
  onEvent: (event: ServerEvent) => void
  onStatus: (status: SocketStatus) => void
  onFatal: (reason: FatalReason) => void
}

const PING_EVERY_MS = 10_000
/** No frame at all for this long means a half-open socket: reconnect. */
const SILENCE_LIMIT_MS = 35_000
/** How long to wait for any reply after an important send before reconnecting. */
const PROBE_TIMEOUT_MS = 5_000
const MAX_BACKOFF_MS = 10_000

export function wsUrl(slug: string): string {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${location.host}/api/rooms/${encodeURIComponent(slug)}/ws`
}

export class RoomSocket {
  readonly clock = new ServerClock()
  private ws: WebSocket | null = null
  private attempts = 0
  private stopped = false
  private ready = false
  private retryTimer: ReturnType<typeof setTimeout> | undefined
  private pingTimer: ReturnType<typeof setInterval> | undefined
  private burstTimers: ReturnType<typeof setTimeout>[] = []
  private lastFrameAt = 0
  private probeTimer: ReturnType<typeof setTimeout> | undefined
  private readonly onOnline = () => {
    if (!this.ready && !this.stopped) this.reconnectNow()
  }

  constructor(
    private readonly url: string,
    private readonly token: string,
    private readonly handlers: SocketHandlers,
  ) {}

  start(): void {
    this.stopped = false
    window.addEventListener('online', this.onOnline)
    this.open()
  }

  /** Closes for good (unmount or leaving). */
  stop(): void {
    this.stopped = true
    window.removeEventListener('online', this.onOnline)
    this.clearTimers()
    this.ws?.close(1000)
    this.ws = null
    this.ready = false
  }

  get isReady(): boolean {
    return this.ready
  }

  send(event: ClientEvent): boolean {
    if (!this.ready || this.ws?.readyState !== WebSocket.OPEN) return false
    this.ws.send(JSON.stringify(event))
    return true
  }

  private open(): void {
    this.handlers.onStatus(this.attempts === 0 ? 'connecting' : 'reconnecting')
    const ws = new WebSocket(this.url)
    this.ws = ws

    ws.onopen = () => ws.send(JSON.stringify({ type: 'auth', token: this.token } satisfies ClientEvent))

    ws.onmessage = (msg) => {
      this.lastFrameAt = Date.now()
      clearTimeout(this.probeTimer)
      let event: ServerEvent
      try {
        event = JSON.parse(msg.data as string) as ServerEvent
      } catch {
        return
      }
      if (event.type === 'pong') {
        this.clock.addSample(event.t, event.server_time, Date.now())
        return
      }
      if (event.type === 'room_state_sync') {
        this.ready = true
        this.attempts = 0
        this.clock.seed(event.server_time)
        this.startPings()
        // Apply the snapshot first so "open" observers see fresh state.
        this.handlers.onEvent(event)
        this.handlers.onStatus('open')
        return
      }
      this.handlers.onEvent(event)
    }

    ws.onclose = (e) => {
      if (this.ws !== ws) return
      this.ready = false
      this.clearTimers()
      const fatal = fatalFor(e.code)
      if (fatal) {
        this.stopped = true
        this.handlers.onStatus('closed')
        this.handlers.onFatal(fatal)
        return
      }
      if (this.stopped) return
      this.scheduleReconnect()
    }
  }

  private scheduleReconnect(): void {
    this.attempts += 1
    this.handlers.onStatus('reconnecting')
    const base = Math.min(MAX_BACKOFF_MS, 500 * 2 ** (this.attempts - 1))
    const delay = base / 2 + Math.random() * (base / 2)
    this.retryTimer = setTimeout(() => this.open(), delay)
  }

  private reconnectNow(): void {
    clearTimeout(this.retryTimer)
    this.ws?.close()
    this.open()
  }

  /**
   * Called right after sending something important: if the server does not
   * answer a ping within a few seconds, the connection is dead (common on
   * tunnels and mobile networks) and we reconnect instead of waiting.
   */
  probe(): void {
    if (!this.ready || this.probeTimer) return
    this.send({ type: 'ping', t: Date.now() })
    this.probeTimer = setTimeout(() => {
      this.probeTimer = undefined
      if (this.ready) this.ws?.close()
    }, PROBE_TIMEOUT_MS)
  }

  private ping(): void {
    if (this.ready && Date.now() - this.lastFrameAt > SILENCE_LIMIT_MS) {
      // The server answers pings; silence means the connection is dead even
      // though the browser still reports it open.
      this.ws?.close()
      return
    }
    this.send({ type: 'ping', t: Date.now() })
  }

  /** A quick burst right after connecting gives a good clock estimate fast. */
  private startPings(): void {
    this.clock.reset()
    this.burstTimers = [0, 250, 500, 1000, 2000].map((d) => setTimeout(() => this.ping(), d))
    this.pingTimer = setInterval(() => this.ping(), PING_EVERY_MS)
  }

  private clearTimers(): void {
    clearTimeout(this.retryTimer)
    clearTimeout(this.probeTimer)
    this.probeTimer = undefined
    clearInterval(this.pingTimer)
    this.burstTimers.forEach(clearTimeout)
    this.burstTimers = []
  }
}

function fatalFor(code: number): FatalReason | null {
  switch (code) {
    case CloseCode.Unauthorized:
      return 'unauthorized'
    case CloseCode.NotFound:
      return 'not_found'
    case CloseCode.RoomFull:
      return 'room_full'
    case CloseCode.Left:
      return 'left'
    case CloseCode.Kicked:
      return 'kicked'
    case CloseCode.TooManyConnections:
      return 'too_many_tabs'
    default:
      return null
  }
}
