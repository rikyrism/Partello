// Room state on the client. `reduce` is pure (unit tested); `createRoomStore`
// wraps it in a tiny external store so components subscribe to slices with
// useSyncExternalStore and a chat message does not re-render the player.

import type {
  ChatMessage,
  Limits,
  IceServer,
  Participant,
  Playback,
  QueueItem,
  RoomInfo,
  ServerEvent,
} from '../realtime/protocol'
import type { SocketStatus } from '../realtime/socket'

export interface SystemEntry {
  kind: 'system'
  id: string
  /** English template, translated at render time: `t(text, vars)`. */
  text: string
  vars?: Record<string, string>
  at: number
}

export interface MessageEntry {
  kind: 'message'
  message: ChatMessage
  /** Optimistic copy waiting for the server echo. */
  pending?: boolean
  failed?: boolean
}

export type ChatEntry = SystemEntry | MessageEntry

export interface RoomState {
  status: SocketStatus
  synced: boolean
  selfId: string | null
  room: RoomInfo | null
  participants: Participant[]
  playback: Playback | null
  queue: QueueItem[]
  chat: ChatEntry[]
  iceServers: IceServer[]
  limits: Limits | null
  /** Who voted to skip the current video. */
  skipVotes: string[]
  /** A 3-2-1 in progress: playback starts at `startAt` (server ms) unless it changes first. */
  countdown: { startAt: number; version: number } | null
}

export const initialRoomState: RoomState = {
  status: 'connecting',
  synced: false,
  selfId: null,
  room: null,
  participants: [],
  playback: null,
  queue: [],
  chat: [],
  iceServers: [],
  limits: null,
  skipVotes: [],
  countdown: null,
}

const CHAT_KEEP = 300

export type RoomAction =
  | { type: 'server'; event: ServerEvent; at?: number }
  | { type: 'status'; status: SocketStatus }
  | { type: 'chat_pending'; message: ChatMessage }
  | { type: 'chat_failed'; nonce: string }

let systemSeq = 0
function system(note: Note, at: number): SystemEntry {
  systemSeq += 1
  return { kind: 'system', id: `sys-${at}-${systemSeq}`, text: note.text, vars: note.vars, at }
}

export interface Note {
  text: string
  vars?: Record<string, string>
}

function appendChat(chat: ChatEntry[], entry: ChatEntry): ChatEntry[] {
  const next = [...chat, entry]
  return next.length > CHAT_KEEP ? next.slice(next.length - CHAT_KEEP) : next
}

/** Human-readable notes for changes worth announcing in the chat timeline. */
export function describeChange(before: Participant, after: Participant, isSelf: boolean): Note[] {
  const name = after.display_name
  const n = (self: string, other: string): Note => (isSelf ? { text: self } : { text: other, vars: { name } })
  const notes: Note[] = []
  if (!before.is_host && after.is_host) notes.push(n('You are now the host', '{name} is now the host'))
  const b = before.voice
  const a = after.voice
  if (!b && a) notes.push(n('You joined the call', '{name} joined the call'))
  else if (b && !a) notes.push(n('You left the call', '{name} left the call'))
  else if (b && a) {
    if (b.camera !== a.camera)
      notes.push(a.camera ? n('You turned the camera on', '{name} turned the camera on') : n('You turned the camera off', '{name} turned the camera off'))
    if (b.muted !== a.muted)
      notes.push(a.muted ? n('You muted the microphone', '{name} muted the microphone') : n('You unmuted the microphone', '{name} unmuted the microphone'))
  }
  return notes
}

export function reduce(state: RoomState, action: RoomAction): RoomState {
  switch (action.type) {
    case 'status':
      return { ...state, status: action.status }

    case 'chat_pending':
      return { ...state, chat: appendChat(state.chat, { kind: 'message', message: action.message, pending: true }) }

    case 'chat_failed':
      return {
        ...state,
        chat: state.chat.map((e) =>
          e.kind === 'message' && e.pending && e.message.nonce === action.nonce ? { ...e, pending: false, failed: true } : e,
        ),
      }

    case 'server':
      return applyServer(state, action.event, action.at ?? Date.now())
  }
}

function applyServer(state: RoomState, ev: ServerEvent, at: number): RoomState {
  switch (ev.type) {
    case 'room_state_sync': {
      // Optimistic messages that reached the server before the connection
      // dropped are in the history now; the rest are offered for re-sending.
      const delivered = (m: ChatMessage) =>
        ev.chat.some((h) => h.participant_id === m.participant_id && h.body === m.body && h.created_at >= m.created_at - 60_000)
      const pending: ChatEntry[] = state.chat
        .filter((e): e is MessageEntry => e.kind === 'message' && !!(e.pending || e.failed))
        .filter((e) => !delivered(e.message))
        .map((e) => ({ ...e, pending: false, failed: true }))
      const history: ChatEntry[] = ev.chat.map((message) => ({ kind: 'message', message }))
      return {
        ...state,
        synced: true,
        selfId: ev.self_id,
        room: ev.room,
        participants: ev.participants,
        playback: ev.playback,
        queue: ev.queue,
        chat: [...history, ...pending],
        iceServers: ev.ice_servers,
        limits: ev.limits,
        skipVotes: ev.skip_votes ?? [],
        countdown: null,
      }
    }

    case 'participant_joined': {
      if (state.participants.some((p) => p.id === ev.participant.id)) return state
      return {
        ...state,
        participants: [...state.participants, ev.participant],
        chat: appendChat(state.chat, system({ text: '{name} joined', vars: { name: ev.participant.display_name } }, at)),
      }
    }

    case 'participant_left': {
      if (!state.participants.some((p) => p.id === ev.participant_id)) return state
      const text = ev.reason === 'kicked' ? '{name} was removed by the host' : '{name} left'
      return {
        ...state,
        participants: state.participants.filter((p) => p.id !== ev.participant_id),
        chat: appendChat(state.chat, system({ text, vars: { name: ev.display_name } }, at)),
      }
    }

    case 'participant_updated': {
      const before = state.participants.find((p) => p.id === ev.participant.id)
      if (!before) return state
      let chat = state.chat
      for (const note of describeChange(before, ev.participant, ev.participant.id === state.selfId)) {
        chat = appendChat(chat, system(note, at))
      }
      return {
        ...state,
        participants: state.participants.map((p) => (p.id === ev.participant.id ? ev.participant : p)),
        chat,
      }
    }

    case 'playback_changed': {
      // Drop out-of-order updates.
      if (state.playback && ev.playback.version < state.playback.version) return state
      // Votes belong to one video.
      const sameItem = state.playback?.item?.id === ev.playback.item?.id
      // Any newer playback state ends a countdown (it started, or was cancelled).
      const countdown = state.countdown && ev.playback.version > state.countdown.version ? null : state.countdown
      return { ...state, playback: ev.playback, skipVotes: sameItem ? state.skipVotes : [], countdown }
    }

    case 'countdown':
      if (state.playback && state.playback.version !== ev.version) return state
      return { ...state, countdown: { startAt: ev.start_at, version: ev.version } }

    case 'skip_votes': {
      if (state.playback?.item?.id !== ev.item_id) return state
      if (!ev.skipped) return { ...state, skipVotes: ev.voters }
      return {
        ...state,
        skipVotes: [],
        chat: appendChat(state.chat, system({ text: 'The room voted to skip {title}', vars: { title: state.playback.item.title } }, at)),
      }
    }

    case 'queue_updated':
      return { ...state, queue: ev.queue }

    case 'chat_message_created': {
      const { message } = ev
      const idx = message.nonce
        ? state.chat.findIndex((e) => e.kind === 'message' && e.pending && e.message.nonce === message.nonce)
        : -1
      if (idx >= 0) {
        const chat = state.chat.slice()
        chat[idx] = { kind: 'message', message }
        return { ...state, chat }
      }
      return { ...state, chat: appendChat(state.chat, { kind: 'message', message }) }
    }

    case 'room_settings_changed':
      return { ...state, room: ev.room }

    default:
      return state
  }
}

export interface RoomStore {
  getState: () => RoomState
  dispatch: (action: RoomAction) => void
  subscribe: (listener: () => void) => () => void
}

export function createRoomStore(initial: RoomState = initialRoomState): RoomStore {
  let state = initial
  const listeners = new Set<() => void>()
  return {
    getState: () => state,
    dispatch(action) {
      const next = reduce(state, action)
      if (next !== state) {
        state = next
        listeners.forEach((l) => l())
      }
    },
    subscribe(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}
