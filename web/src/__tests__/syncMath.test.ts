import { describe, expect, it } from 'vitest'
import { decideCorrection, detectJump, expectedPosition, playerErrorMessage, CORRECTION_COOLDOWN_MS } from '../features/player/syncMath'
import { playback } from './fixtures'

describe('expectedPosition', () => {
  it('advances while playing and holds while paused', () => {
    expect(expectedPosition(playback(), 1_005_000)).toBe(15)
    expect(expectedPosition(playback({ status: 'paused' }), 1_005_000)).toBe(10)
  })
  it('never exceeds the duration or goes negative', () => {
    expect(expectedPosition(playback({ duration: 12 }), 1_100_000)).toBe(12)
    expect(expectedPosition(playback({ position: 0 }), 900_000)).toBe(0)
  })
})

describe('detectJump', () => {
  it('flags a scrub but not normal playback or a buffering stall', () => {
    expect(detectJump(10, 0, 10.5, 500, true)).toBe(false)
    expect(detectJump(10, 0, 10, 2_000, true)).toBe(false) // stalled while buffering
    expect(detectJump(10, 0, 40, 500, true)).toBe(true)
    expect(detectJump(10, 0, 3, 500, false)).toBe(true)
  })
})

describe('decideCorrection', () => {
  const base = { playback: playback(), serverNow: 1_000_000, canControl: true, userJumped: false, msSinceLastCorrection: CORRECTION_COOLDOWN_MS }

  it('does nothing when in sync', () => {
    expect(decideCorrection({ ...base, localTime: 10.4 })).toEqual({ kind: 'none' })
  })
  it('seeks back to the room when drifting', () => {
    expect(decideCorrection({ ...base, localTime: 13 })).toEqual({ kind: 'seek', to: 10 })
  })
  it('rate-limits corrections', () => {
    expect(decideCorrection({ ...base, localTime: 13, msSinceLastCorrection: 100 })).toEqual({ kind: 'none' })
  })
  it('turns a controller scrub into a room seek', () => {
    expect(decideCorrection({ ...base, localTime: 60, userJumped: true })).toEqual({ kind: 'user_seek', to: 60 })
  })
  it('reverts a scrub by someone who may not control', () => {
    expect(decideCorrection({ ...base, localTime: 60, userJumped: true, canControl: false })).toEqual({ kind: 'seek', to: 10 })
  })
  it('is stricter while paused', () => {
    expect(decideCorrection({ ...base, playback: playback({ status: 'paused' }), localTime: 10.8 })).toEqual({ kind: 'seek', to: 10 })
  })
  it('ignores rooms with nothing loaded', () => {
    expect(decideCorrection({ ...base, playback: playback({ item: null, status: 'idle' }), localTime: 99 })).toEqual({ kind: 'none' })
  })
})

it('explains embed errors in plain words', () => {
  expect(playerErrorMessage(150)).toMatch(/doesn't allow/)
  expect(playerErrorMessage(100)).toMatch(/removed or is private/)
})
