import type { Participant, Playback, QueueItem, ServerEvent } from '../realtime/protocol'

export const host: Participant = { id: 'a-host', display_name: 'Ana', is_host: true, connected: true, joined_at: 1, voice: null }
export const guest: Participant = { id: 'b-guest', display_name: 'Budi', is_host: false, connected: true, joined_at: 2, voice: null }

export const item: QueueItem = {
  id: 'item-1',
  video_id: 'dQw4w9WgXcQ',
  title: 'A video',
  author: 'Someone',
  thumbnail_url: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg',
  start: null,
  added_by: host.id,
  added_by_name: host.display_name,
}

export function playback(over: Partial<Playback> = {}): Playback {
  return { item, status: 'playing', position: 10, updated_at: 1_000_000, version: 1, duration: 200, ...over }
}

export const room = {
  slug: 'abcdefghijkm',
  name: "Ana's room",
  settings: { control_mode: 'host_only', queue_mode: 'everyone' },
  max_participants: 16,
  created_at: 0,
  has_password: false,
} as const satisfies import('../realtime/protocol').RoomInfo

export function sync(selfId = host.id, over: Partial<Extract<ServerEvent, { type: 'room_state_sync' }>> = {}): ServerEvent {
  return {
    type: 'room_state_sync',
    self_id: selfId,
    room: { ...room, settings: { ...room.settings } },
    participants: [host, guest],
    playback: playback(),
    queue: [],
    chat: [],
    ice_servers: [],
    limits: { chat_max_chars: 500, max_voice_participants: 8, max_queue_len: 100, search_enabled: true },
    server_time: 1_000_000,
    ...over,
  }
}
