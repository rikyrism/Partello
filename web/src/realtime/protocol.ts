// Wire types for the Partello WebSocket protocol.
// Mirrors server/src/realtime/protocol.rs; documented in docs/WEBSOCKET.md.

export type ControlMode = 'host_only' | 'everyone'
export type QueueMode = 'host_only' | 'everyone'

export interface RoomSettings {
  control_mode: ControlMode
  queue_mode: QueueMode
}

export interface RoomInfo {
  slug: string
  name: string
  settings: RoomSettings
  max_participants: number
  created_at: number
  has_password: boolean
}

export interface VoiceState {
  muted: boolean
  camera: boolean
}

export interface Participant {
  id: string
  display_name: string
  is_host: boolean
  connected: boolean
  joined_at: number
  voice: VoiceState | null
}

export interface QueueItem {
  id: string
  video_id: string
  title: string
  author: string | null
  thumbnail_url: string
  start: number | null
  added_by: string
  added_by_name: string
}

export type PlayStatus = 'idle' | 'playing' | 'paused' | 'ended'

export interface Playback {
  item: QueueItem | null
  status: PlayStatus
  /** Seconds into the video at `updated_at`. */
  position: number
  /** Server clock, Unix ms. */
  updated_at: number
  version: number
  duration: number | null
}

export interface ChatMessage {
  id: string
  participant_id: string
  display_name: string
  body: string
  created_at: number
  nonce?: string
}

export interface IceServer {
  urls: string[]
  username?: string
  credential?: string
}

export interface Limits {
  chat_max_chars: number
  max_voice_participants: number
  max_queue_len: number
  search_enabled: boolean
}

export type SignalData =
  | { kind: 'description'; description: RTCSessionDescriptionInit }
  | { kind: 'candidate'; candidate: RTCIceCandidateInit | null }

export type ServerEvent =
  | {
      type: 'room_state_sync'
      self_id: string
      room: RoomInfo
      participants: Participant[]
      playback: Playback
      queue: QueueItem[]
      chat: ChatMessage[]
      ice_servers: IceServer[]
      limits: Limits
      server_time: number
    }
  | { type: 'participant_joined'; participant: Participant }
  | { type: 'participant_left'; participant_id: string; display_name: string; reason: 'left' | 'disconnected' | 'kicked' }
  | { type: 'participant_updated'; participant: Participant }
  | { type: 'playback_changed'; playback: Playback; actor_id: string | null; server_time: number; nonce?: string }
  | { type: 'queue_updated'; queue: QueueItem[]; actor_id: string | null; nonce?: string }
  | { type: 'chat_message_created'; message: ChatMessage }
  | { type: 'voice_signal'; from: string; data: SignalData }
  | { type: 'room_settings_changed'; room: RoomInfo }
  | { type: 'pong'; t: number; server_time: number }
  | { type: 'error'; code: string; message: string; nonce?: string }

export type PlaybackAction = 'play' | 'pause' | 'seek'

export type ClientEvent =
  | { type: 'auth'; token: string }
  | { type: 'ping'; t: number }
  | { type: 'chat_send'; body: string; nonce?: string }
  | { type: 'playback_control'; action: PlaybackAction; position: number }
  | { type: 'playback_duration'; item_id: string; duration: number }
  | { type: 'playback_ended'; item_id: string }
  | { type: 'queue_add'; url: string; nonce?: string }
  | { type: 'queue_remove'; item_id: string }
  | { type: 'queue_play'; item_id: string }
  | { type: 'queue_move'; item_id: string; to_index: number }
  | { type: 'queue_skip' }
  | { type: 'voice_join'; muted: boolean; camera: boolean }
  | { type: 'voice_leave' }
  | { type: 'voice_update'; muted: boolean; camera: boolean }
  | { type: 'voice_signal'; to: string; data: SignalData }
  | { type: 'host_transfer'; to: string }
  | { type: 'kick'; participant_id: string }
  | { type: 'leave' }

/** Application close codes sent by the server. */
export const CloseCode = {
  Left: 4000,
  Kicked: 4003,
  TooSlow: 4008,
  BadRequest: 4400,
  Unauthorized: 4401,
  NotFound: 4404,
  RoomFull: 4409,
  TooManyConnections: 4429,
} as const
