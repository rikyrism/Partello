import { describe, expect, it } from 'vitest'
import { initialRoomState, reduce, type RoomState } from '../room/store'
import { translate } from '../i18n'
import { guest, host, playback, sync } from './fixtures'

const synced = (): RoomState => reduce(initialRoomState, { type: 'server', event: sync() })

describe('room store', () => {
  it('applies the snapshot', () => {
    const s = synced()
    expect(s.synced).toBe(true)
    expect(s.selfId).toBe(host.id)
    expect(s.participants).toHaveLength(2)
  })

  it('announces joins and leaves in the chat timeline', () => {
    let s = synced()
    s = reduce(s, { type: 'server', event: { type: 'participant_joined', participant: { ...guest, id: 'c', display_name: 'Citra' } } })
    s = reduce(s, { type: 'server', event: { type: 'participant_left', participant_id: 'c', display_name: 'Citra', reason: 'left' } })
    expect(s.participants).toHaveLength(2)
    expect(s.chat.map((e) => (e.kind === 'system' ? translate(e.text, e.vars) : ''))).toEqual(['Citra joined', 'Citra left'])
    expect(s.chat.map((e) => (e.kind === 'system' ? translate(e.text, e.vars, 'id') : ''))).toEqual(['Citra bergabung', 'Citra keluar'])
  })

  it('describes call and camera changes', () => {
    let s = synced()
    s = reduce(s, { type: 'server', event: { type: 'participant_updated', participant: { ...guest, voice: { muted: false, camera: false } } } })
    s = reduce(s, { type: 'server', event: { type: 'participant_updated', participant: { ...guest, voice: { muted: true, camera: true } } } })
    const texts = s.chat.map((e) => (e.kind === 'system' ? translate(e.text, e.vars) : ''))
    expect(texts).toEqual(['Budi joined the call', 'Budi turned the camera on', 'Budi muted the microphone'])
  })

  it('drops out-of-order playback updates', () => {
    let s = synced()
    s = reduce(s, { type: 'server', event: { type: 'playback_changed', playback: playback({ version: 5, status: 'paused' }), actor_id: null, server_time: 0 } })
    s = reduce(s, { type: 'server', event: { type: 'playback_changed', playback: playback({ version: 4 }), actor_id: null, server_time: 0 } })
    expect(s.playback?.status).toBe('paused')
  })

  it('reconciles optimistic chat by nonce and keeps unsent messages across reconnects', () => {
    let s = synced()
    const msg = { id: 'tmp', participant_id: host.id, display_name: 'Ana', body: 'hi', created_at: 1, nonce: 'n1' }
    s = reduce(s, { type: 'chat_pending', message: msg })
    s = reduce(s, { type: 'server', event: { type: 'chat_message_created', message: { ...msg, id: 'real' } } })
    expect(s.chat).toHaveLength(1)
    expect(s.chat[0]).toMatchObject({ kind: 'message', message: { id: 'real' } })
    expect((s.chat[0] as { pending?: boolean }).pending).toBeUndefined()

    s = reduce(s, { type: 'chat_pending', message: { ...msg, nonce: 'n2', body: 'still sending' } })
    s = reduce(s, { type: 'chat_failed', nonce: 'n2' })
    s = reduce(s, { type: 'server', event: sync(host.id, { chat: [{ ...msg, id: 'real', nonce: undefined }] }) })
    expect(s.chat).toHaveLength(2)
    expect(s.chat[1]).toMatchObject({ failed: true })
  })
})

describe('chat across a dropped connection', () => {
  it('drops optimistic messages that reached the server and flags the rest', () => {
    let s = synced()
    const base = { participant_id: host.id, display_name: 'Ana', created_at: 1_000 }
    s = reduce(s, { type: 'chat_pending', message: { ...base, id: 'p1', body: 'made it', nonce: 'a' } })
    s = reduce(s, { type: 'chat_pending', message: { ...base, id: 'p2', body: 'lost', nonce: 'b' } })
    s = reduce(s, { type: 'server', event: sync(host.id, { chat: [{ ...base, id: 'real', body: 'made it', created_at: 1_200 }] }) })
    expect(s.chat).toHaveLength(2)
    expect(s.chat[0]).toMatchObject({ message: { id: 'real' } })
    expect(s.chat[1]).toMatchObject({ failed: true, message: { body: 'lost' } })
  })
})
