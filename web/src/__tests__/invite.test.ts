import { describe, expect, it } from 'vitest'
import { inviteMessage } from '../lib/invite'

const base = { inviter: 'Nita', roomName: "Nita's room", link: 'https://partello.app/r/PPBrQJNrbQTe', hasPin: false }

describe('inviteMessage', () => {
  it('builds a paste-ready English invitation', () => {
    expect(inviteMessage({ ...base, nowPlaying: 'The SpongeBob Movie', lang: 'en' })).toBe(
      [
        'Nita is inviting you to watch together on Partello 🍿',
        '',
        "Room: Nita's room",
        'Join: https://partello.app/r/PPBrQJNrbQTe',
        '',
        'Now playing: The SpongeBob Movie',
        '',
        'No account or app needed. Open the link, type your name, and you are in.',
      ].join('\n'),
    )
  })

  it('includes the PIN when known, or says whom to ask', () => {
    expect(inviteMessage({ ...base, hasPin: true, pin: '0420', lang: 'en' })).toContain('PIN: 0420')
    expect(inviteMessage({ ...base, hasPin: true, pin: null, lang: 'en' })).toContain('PIN: ask Nita')
    expect(inviteMessage({ ...base, hasPin: false, pin: '0420', lang: 'en' })).not.toContain('PIN')
  })

  it('speaks Indonesian when the UI does', () => {
    const msg = inviteMessage({ ...base, hasPin: true, pin: '0420', nowPlaying: 'Wildwood', lang: 'id' })
    expect(msg).toContain('Nita mengajak kamu nonton bareng di Partello')
    expect(msg).toContain('Gabung: https://partello.app/r/PPBrQJNrbQTe')
    expect(msg).toContain('PIN: 0420')
    expect(msg).toContain('Sedang diputar: Wildwood')
  })
})
