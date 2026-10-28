import { describe, expect, it } from 'vitest'
import { FEATURED_COUNT, toPicks } from '../pages/landing/trending'
import { shortTitle } from '../pages/landing/genres'
import { mentionQuery, mentions, splitMentions } from '../features/chat/mentions'
import { votesNeeded } from '../features/player/SkipVote'
import { playlistIdOf } from '../lib/youtube'
import { initialRoomState, reduce, type RoomState } from '../room/store'
import { translate } from '../i18n'
import { guest, item, playback, sync } from './fixtures'

const synced = (): RoomState => reduce(initialRoomState, { type: 'server', event: sync() })

describe('mentions', () => {
  it('finds a mention of a name, case-insensitively, at a word boundary', () => {
    expect(mentions('hey @nita look', 'Nita')).toBe(true)
    expect(mentions('@Nita!', 'Nita')).toBe(true)
    expect(mentions('email@nita.com', 'Nita')).toBe(false)
    expect(mentions('@Nitaa', 'Nita')).toBe(false)
    expect(mentions('@Ana Maria come', 'Ana Maria')).toBe(true)
    expect(mentions('hi', '')).toBe(false)
  })

  it('splits text and mentions, longest name first', () => {
    expect(splitMentions('yo @ana maria and @Ana.', ['Ana', 'Ana Maria'])).toEqual([
      { text: 'yo ' },
      { text: '@ana maria', mention: 'Ana Maria' },
      { text: ' and ' },
      { text: '@Ana', mention: 'Ana' },
      { text: '.' },
    ])
    expect(splitMentions('no mentions here', ['Ana'])).toEqual([{ text: 'no mentions here' }])
    expect(splitMentions('weird (name) @a+b', ['a+b'])).toEqual([{ text: 'weird (name) ' }, { text: '@a+b', mention: 'a+b' }])
  })

  it('reads the name being typed', () => {
    expect(mentionQuery('hello @Ri')).toBe('Ri')
    expect(mentionQuery('@')).toBe('')
    expect(mentionQuery('hello @Riky ')).toBeNull()
    expect(mentionQuery('a@b')).toBeNull()
  })
})

describe('playlists', () => {
  it('finds regular playlists but not mixes', () => {
    expect(playlistIdOf('https://www.youtube.com/playlist?list=PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG')).toBe('PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG')
    expect(playlistIdOf('https://youtube.com/watch?v=dQw4w9WgXcQ&list=PLabc123&index=2')).toBe('PLabc123')
    expect(playlistIdOf('https://www.youtube.com/watch?v=YEhzWw8qQWk&list=RDYEhzWw8qQWk&start_radio=1')).toBeNull()
    expect(playlistIdOf('https://youtu.be/dQw4w9WgXcQ')).toBeNull()
  })
})

describe('skip votes', () => {
  it('needs a majority of the people here', () => {
    expect([1, 2, 3, 4, 5].map(votesNeeded)).toEqual([1, 2, 2, 3, 3])
  })

  it('tracks votes for the current video and notes a skip', () => {
    let s = synced()
    s = reduce(s, { type: 'server', event: { type: 'skip_votes', item_id: item.id, voters: [guest.id], skipped: false } })
    expect(s.skipVotes).toEqual([guest.id])
    // Votes for another video are ignored.
    s = reduce(s, { type: 'server', event: { type: 'skip_votes', item_id: 'other', voters: [], skipped: false } })
    expect(s.skipVotes).toEqual([guest.id])
    s = reduce(s, { type: 'server', event: { type: 'skip_votes', item_id: item.id, voters: [guest.id, 'x'], skipped: true } })
    expect(s.skipVotes).toEqual([])
    const note = s.chat.at(-1)
    expect(note?.kind === 'system' && translate(note.text, note.vars)).toBe('The room voted to skip A video')
  })

  it('clears votes when the video changes', () => {
    let s = synced()
    s = reduce(s, { type: 'server', event: { type: 'skip_votes', item_id: item.id, voters: [guest.id], skipped: false } })
    const next = playback({ item: { ...item, id: 'item-2' }, version: 2 })
    s = reduce(s, { type: 'server', event: { type: 'playback_changed', playback: next, actor_id: null, server_time: 0 } })
    expect(s.skipVotes).toEqual([])
  })
})

describe('landing picks', () => {
  const video = (id: string) => ({ id, title: `${id} | Official Video`, channel: 'C', thumb: `https://i.ytimg.com/vi/${id}/maxresdefault.jpg`, duration: 200 })

  it('draws distinct featured videos from today’s chart', () => {
    const picks = toPicks({
      region: 'US',
      fetched_at: 1,
      genres: [
        { name: 'Trending', videos: ['a', 'b', 'c', 'd'].map(video) },
        { name: 'Music', videos: ['c', 'd', 'e', 'f'].map(video) },
        { name: 'Sports', videos: [] },
      ],
    })
    expect(picks.live).toBe(true)
    expect(picks.genres.map((g) => g.name)).toEqual(['Trending', 'Music'])
    expect(picks.featured).toHaveLength(FEATURED_COUNT)
    expect(new Set(picks.featured.map((v) => v.id)).size).toBe(FEATURED_COUNT)
  })

  it('falls back to the built-in picks when the chart is empty or too small', () => {
    expect(toPicks({ region: 'US', fetched_at: 0, genres: [] }).live).toBe(false)
    const small = toPicks({ region: 'US', fetched_at: 1, genres: [{ name: 'Music', videos: ['a', 'b'].map(video) }] })
    expect(small.live).toBe(false)
    expect(small.featured).toHaveLength(FEATURED_COUNT)
  })

  it('shortens chart titles for captions', () => {
    expect(shortTitle('Wildwood – Official Trailer')).toBe('Wildwood')
    expect(shortTitle('Avengers: Doomsday | Official Trailer | Marvel')).toBe('Avengers: Doomsday')
    expect(shortTitle('Plain title')).toBe('Plain title')
  })
})
