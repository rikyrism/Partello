import { describe, expect, it } from 'vitest'
import { splitTimestamps } from '../features/chat/timestamps'
import { initialRoomState, reduce, type RoomState } from '../room/store'
import { playback, sync } from './fixtures'

describe('chat timestamps', () => {
  it('finds m:ss and h:mm:ss', () => {
    expect(splitTimestamps('look at 12:45 lol')).toEqual([{ text: 'look at ' }, { text: '12:45', seconds: 765 }, { text: ' lol' }])
    expect(splitTimestamps('📌 1:02:03')).toEqual([{ text: '📌 ' }, { text: '1:02:03', seconds: 3723 }])
    expect(splitTimestamps('0:07 and 3:30')).toEqual([{ text: '0:07', seconds: 7 }, { text: ' and ' }, { text: '3:30', seconds: 210 }])
  })

  it('leaves other numbers alone', () => {
    for (const text of ['no times', 'ratio 1:2', '12:61', '1:2:03', 'v1.2:30x', '123:45']) {
      expect(splitTimestamps(text).every((s) => s.seconds === undefined), text).toBe(true)
    }
  })
})

describe('countdown', () => {
  const synced = (): RoomState => reduce(initialRoomState, { type: 'server', event: sync() })

  it('starts on the paused version and ends with the next playback change', () => {
    let s = synced()
    const paused = playback({ status: 'paused', version: 2 })
    s = reduce(s, { type: 'server', event: { type: 'playback_changed', playback: paused, actor_id: 'a-host', server_time: 0 } })
    s = reduce(s, { type: 'server', event: { type: 'countdown', start_at: 5_000, version: 2, actor_id: 'a-host' } })
    expect(s.countdown).toEqual({ startAt: 5_000, version: 2 })
    s = reduce(s, { type: 'server', event: { type: 'playback_changed', playback: playback({ version: 3 }), actor_id: null, server_time: 0 } })
    expect(s.countdown).toBeNull()
  })

  it('ignores a countdown for a playback state that already changed', () => {
    let s = synced() // version 1
    s = reduce(s, { type: 'server', event: { type: 'countdown', start_at: 5_000, version: 0, actor_id: 'a-host' } })
    expect(s.countdown).toBeNull()
  })
})
