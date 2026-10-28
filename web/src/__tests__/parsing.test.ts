import { describe, expect, it } from 'vitest'
import { parseYouTubeId } from '../lib/youtube'
import { roomSlugFrom } from '../pages/landing/useCreateRoom'
import { ServerClock } from '../realtime/clock'
import { formatTime, initials } from '../lib/format'

describe('parseYouTubeId', () => {
  it.each([
    'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
    'youtube.com/watch?v=dQw4w9WgXcQ&t=30',
    'https://youtu.be/dQw4w9WgXcQ',
    'https://www.youtube.com/shorts/dQw4w9WgXcQ',
    'https://m.youtube.com/watch?v=dQw4w9WgXcQ',
  ])('accepts %s', (url) => expect(parseYouTubeId(url)).toBe('dQw4w9WgXcQ'))

  it.each(['', 'hello', 'https://vimeo.com/1', 'https://youtube.com.evil.io/watch?v=dQw4w9WgXcQ', 'javascript:alert(1)', 'https://youtu.be/short'])(
    'rejects %s',
    (url) => expect(parseYouTubeId(url)).toBeNull(),
  )
})

describe('roomSlugFrom', () => {
  it('reads invite links and bare codes', () => {
    expect(roomSlugFrom('https://partello.app/r/abcDEF234567')).toBe('abcDEF234567')
    expect(roomSlugFrom('  abcDEF234567 ')).toBe('abcDEF234567')
    expect(roomSlugFrom('https://partello.app/r/abc')).toBeNull()
    expect(roomSlugFrom('nope')).toBeNull()
  })
})

describe('ServerClock', () => {
  it('prefers the sample with the lowest round trip', () => {
    const c = new ServerClock()
    c.addSample(0, 10_050, 100) // rtt 100 → offset 10_000
    c.addSample(1_000, 12_000, 1_600) // rtt 600 → offset 10_700
    expect(c.offsetMs).toBe(10_000)
  })
})

it('formats times and initials', () => {
  expect(formatTime(65)).toBe('1:05')
  expect(formatTime(3725)).toBe('1:02:05')
  expect(formatTime(NaN)).toBe('0:00')
  expect(initials('ana maria lopez')).toBe('AL')
})
