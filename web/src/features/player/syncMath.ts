// Pure playback-sync math, shared by the player and its tests.

import type { Playback } from '../../realtime/protocol'

/** Local drift we tolerate before seeking (seconds). Seeking re-buffers, so not too eager. */
export const DRIFT_TOLERANCE = 1.0
/** A jump this large while the user has the player focused is treated as their seek. */
export const USER_SEEK_THRESHOLD = 2.0
/** Minimum time between two automatic corrections (ms). */
export const CORRECTION_COOLDOWN_MS = 2_500

/** Where the room's video should be right now, in seconds. */
export function expectedPosition(p: Playback, serverNow: number): number {
  let pos = p.position
  if (p.status === 'playing') pos += Math.max(0, serverNow - p.updated_at) / 1000
  if (p.duration != null) pos = Math.min(pos, p.duration)
  return Math.max(0, pos)
}

export type Correction = { kind: 'none' } | { kind: 'seek'; to: number } | { kind: 'user_seek'; to: number }

/**
 * True when the local player time jumped between two samples, i.e. somebody
 * scrubbed the YouTube timeline. Buffering makes time stall, never jump, so a
 * slow connection is not mistaken for a seek.
 */
export function detectJump(prevTime: number, prevAt: number, curTime: number, now: number, wasPlaying: boolean): boolean {
  const expected = prevTime + (wasPlaying ? Math.max(0, now - prevAt) / 1000 : 0)
  return Math.abs(curTime - expected) > USER_SEEK_THRESHOLD
}

export interface CorrectionInput {
  playback: Playback
  serverNow: number
  localTime: number
  /** The local timeline jumped while the user had the YouTube iframe focused. */
  userJumped: boolean
  canControl: boolean
  msSinceLastCorrection: number
}

/**
 * Decides what to do about the gap between the local player and the room.
 * - A jump made by someone allowed to control becomes a room-wide seek.
 * - Otherwise drift beyond tolerance is corrected locally (rate-limited),
 *   which also reverts seeks by people who may not control playback.
 */
export function decideCorrection(i: CorrectionInput): Correction {
  if (i.playback.status === 'idle' || !i.playback.item) return { kind: 'none' }
  const target = expectedPosition(i.playback, i.serverNow)
  const abs = Math.abs(i.localTime - target)
  if (i.userJumped && i.canControl && abs > USER_SEEK_THRESHOLD) {
    return { kind: 'user_seek', to: i.localTime }
  }
  const tolerance = i.playback.status === 'playing' ? DRIFT_TOLERANCE : 0.5
  if (abs > tolerance && i.msSinceLastCorrection >= CORRECTION_COOLDOWN_MS) {
    return { kind: 'seek', to: target }
  }
  return { kind: 'none' }
}

/** YouTube IFrame API error codes, translated for people. */
export function playerErrorMessage(code: number): string {
  switch (code) {
    case 2:
      return 'This video link is invalid.'
    case 5:
      return "This video can't be played in your browser."
    case 100:
      return 'This video was removed or is private.'
    case 101:
    case 150:
    case 153:
      return "The owner of this video doesn't allow playing it on other sites."
    default:
      return "This video can't be played right now."
  }
}
