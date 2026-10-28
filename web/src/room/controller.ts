// One RoomController per mounted room page: owns the socket and the store and
// exposes intent-level actions. UI components never touch the WebSocket.

import type { ChatMessage, ClientEvent, PlaybackAction, ReplyRef, ServerEvent, SignalData } from '../realtime/protocol'
import { RoomSocket, wsUrl, type FatalReason } from '../realtime/socket'
import { createRoomStore, type RoomStore } from './store'
import { translate } from '../i18n'

export interface ControllerHandlers {
  onFatal: (reason: FatalReason) => void
  /** Server-side rejection of something this user tried. */
  onError: (message: string, code: string) => void
}

type SignalListener = (from: string, data: SignalData) => void
type ErrorListener = (code: string, message: string) => void
type ReactionListener = (participantId: string, emoji: string) => void
type CallTokenListener = (url: string, token: string) => void
type MessageListener = (message: ChatMessage) => void

let nonceSeq = 0
const newNonce = () => `${Date.now().toString(36)}-${(nonceSeq++).toString(36)}`

export class RoomController {
  readonly store: RoomStore = createRoomStore()
  readonly socket: RoomSocket
  private signalListeners = new Set<SignalListener>()
  private errorListeners = new Set<ErrorListener>()
  private reactionListeners = new Set<ReactionListener>()
  private callTokenListeners = new Set<CallTokenListener>()
  private messageListeners = new Set<MessageListener>()
  private pendingAdds = new Map<string, { url: string; playlist: boolean; resolve: () => void; reject: (e: Error) => void }>()

  private handlers: ControllerHandlers = { onFatal: () => {}, onError: () => {} }

  constructor(
    readonly slug: string,
    readonly token: string,
  ) {
    this.socket = new RoomSocket(wsUrl(slug), token, {
      onEvent: (e) => this.onEvent(e),
      onStatus: (status) => {
        this.store.dispatch({ type: 'status', status })
        // Requests sent on a connection that died are sent again; the server
        // recognises the nonce, so nothing is added twice.
        if (status === 'open') this.resendPendingAdds()
      },
      onFatal: (reason) => {
        this.failPendingAdds('Disconnected')
        this.handlers.onFatal(reason)
      },
    })
  }

  /** UI callbacks; set (and refreshed) by the page that owns the controller. */
  setHandlers(handlers: ControllerHandlers): void {
    this.handlers = handlers
  }

  get clock() {
    return this.socket.clock
  }

  get selfId(): string | null {
    return this.store.getState().selfId
  }

  start(): void {
    this.socket.start()
  }

  stop(): void {
    this.failPendingAdds('Disconnected')
    this.socket.stop()
  }

  /** Server rejections, for features that must undo optimistic state (the call). */
  onServerError(listener: ErrorListener): () => void {
    this.errorListeners.add(listener)
    return () => this.errorListeners.delete(listener)
  }

  onSignal(listener: SignalListener): () => void {
    this.signalListeners.add(listener)
    return () => this.signalListeners.delete(listener)
  }

  /** Emoji reactions as they happen (not kept in the store). */
  onReaction(listener: ReactionListener): () => void {
    this.reactionListeners.add(listener)
    return () => this.reactionListeners.delete(listener)
  }

  /** SFU credentials, sent after the server admitted us to the call. */
  onCallToken(listener: CallTokenListener): () => void {
    this.callTokenListeners.add(listener)
    return () => this.callTokenListeners.delete(listener)
  }

  /** New chat messages from other people (for sounds and mentions). */
  onMessage(listener: MessageListener): () => void {
    this.messageListeners.add(listener)
    return () => this.messageListeners.delete(listener)
  }

  private onEvent(event: ServerEvent): void {
    switch (event.type) {
      case 'voice_signal':
        this.signalListeners.forEach((l) => l(event.from, event.data))
        return
      case 'reaction':
        this.reactionListeners.forEach((l) => l(event.participant_id, event.emoji))
        return
      case 'call_token':
        this.callTokenListeners.forEach((l) => l(event.url, event.token))
        return
      case 'chat_message_created':
        if (event.message.participant_id !== this.selfId) this.messageListeners.forEach((l) => l(event.message))
        break
      case 'error': {
        this.errorListeners.forEach((l) => l(event.code, event.message))
        if (event.code === 'kicked') return // the close frame follows
        if (event.nonce && this.pendingAdds.has(event.nonce)) {
          this.pendingAdds.get(event.nonce)!.reject(new Error(translate(event.message)))
          this.pendingAdds.delete(event.nonce)
          return
        }
        if (event.nonce) this.store.dispatch({ type: 'chat_failed', nonce: event.nonce })
        this.handlers.onError(translate(event.message), event.code)
        return
      }
      case 'queue_updated':
      case 'playback_changed':
        // The server echoes the nonce of the queue_add this change completes.
        if (event.nonce && this.pendingAdds.has(event.nonce)) {
          this.pendingAdds.get(event.nonce)!.resolve()
          this.pendingAdds.delete(event.nonce)
        }
        break
    }
    this.store.dispatch({ type: 'server', event })
  }

  private resendPendingAdds(): void {
    this.pendingAdds.forEach((p, nonce) =>
      this.send({ type: p.playlist ? 'queue_add_playlist' : 'queue_add', url: p.url, nonce }),
    )
  }

  private failPendingAdds(message: string): void {
    this.pendingAdds.forEach((p) => p.reject(new Error(message)))
    this.pendingAdds.clear()
  }

  private send(event: ClientEvent): boolean {
    return this.socket.send(event)
  }

  // ------------------------------------------------------------- chat

  sendChat(body: string, replyTo?: ReplyRef): boolean {
    const trimmed = body.trim()
    const selfId = this.selfId
    if (!trimmed || !selfId) return false
    const nonce = newNonce()
    const self = this.store.getState().participants.find((p) => p.id === selfId)
    this.store.dispatch({
      type: 'chat_pending',
      message: {
        id: `pending-${nonce}`,
        participant_id: selfId,
        display_name: self?.display_name ?? translate('You'),
        body: trimmed,
        created_at: this.clock.now(),
        reply_to: replyTo
          ? { id: replyTo.id, participant_id: replyTo.participant_id, display_name: replyTo.display_name, body: replyTo.body }
          : undefined,
        nonce,
      },
    })
    const sent = this.send({ type: 'chat_send', body: trimmed, nonce, reply_to: replyTo?.id })
    if (!sent) this.store.dispatch({ type: 'chat_failed', nonce })
    else this.socket.probe()
    return sent
  }

  // ------------------------------------------------------------- playback

  control(action: PlaybackAction, position: number): void {
    this.send({ type: 'playback_control', action, position: Math.max(0, position) })
  }

  reportDuration(itemId: string, duration: number): void {
    this.send({ type: 'playback_duration', item_id: itemId, duration })
  }

  reportEnded(itemId: string): void {
    this.send({ type: 'playback_ended', item_id: itemId })
  }

  // ------------------------------------------------------------- queue

  queueAdd(url: string, playlist = false): Promise<void> {
    const nonce = newNonce()
    return new Promise((resolve, reject) => {
      if (!this.send({ type: playlist ? 'queue_add_playlist' : 'queue_add', url, nonce })) {
        reject(new Error(translate('Not connected. Try again in a moment.')))
        return
      }
      this.pendingAdds.set(nonce, { url, playlist, resolve, reject })
      this.socket.probe()
      setTimeout(() => {
        if (this.pendingAdds.has(nonce)) {
          this.pendingAdds.get(nonce)!.reject(new Error(translate('No answer from the server. Check your connection and try again.')))
          this.pendingAdds.delete(nonce)
        }
      }, 20_000)
    })
  }

  queueRemove(itemId: string): void {
    this.send({ type: 'queue_remove', item_id: itemId })
  }

  queuePlay(itemId: string): void {
    this.send({ type: 'queue_play', item_id: itemId })
  }

  queueMove(itemId: string, toIndex: number): void {
    this.send({ type: 'queue_move', item_id: itemId, to_index: toIndex })
  }

  queueSkip(): void {
    this.send({ type: 'queue_skip' })
  }

  skipVote(itemId: string, vote: boolean): void {
    this.send({ type: 'skip_vote', item_id: itemId, vote })
  }

  react(emoji: string): void {
    this.send({ type: 'react', emoji })
  }

  // ------------------------------------------------------------- call

  voiceJoin(muted: boolean, camera: boolean): boolean {
    return this.send({ type: 'voice_join', muted, camera })
  }

  voiceUpdate(muted: boolean, camera: boolean): void {
    this.send({ type: 'voice_update', muted, camera })
  }

  voiceLeave(): void {
    this.send({ type: 'voice_leave' })
  }

  sendSignal(to: string, data: SignalData): void {
    this.send({ type: 'voice_signal', to, data })
  }

  // ------------------------------------------------------------- people

  kick(participantId: string): void {
    this.send({ type: 'kick', participant_id: participantId })
  }

  hostTransfer(to: string): void {
    this.send({ type: 'host_transfer', to })
  }

  leave(): void {
    this.send({ type: 'leave' })
  }
}
