// Short notification tones made with Web Audio, so there are no sound files.

import { audioContext } from '../call/speaking'

let lastAt = 0

/** A soft blip for a new message, two rising notes for a mention. Throttled. */
export function playChatSound(kind: 'message' | 'mention'): void {
  const now = Date.now()
  if (now - lastAt < 1200) return
  lastAt = now
  const ctx = audioContext()
  if (!ctx || ctx.state !== 'running') return
  const notes = kind === 'mention' ? [660, 990] : [740]
  notes.forEach((freq, i) => {
    const start = ctx.currentTime + i * 0.12
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    osc.type = 'sine'
    osc.frequency.value = freq
    gain.gain.setValueAtTime(0.0001, start)
    gain.gain.exponentialRampToValueAtTime(0.12, start + 0.015)
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.18)
    osc.connect(gain).connect(ctx.destination)
    osc.start(start)
    osc.stop(start + 0.2)
  })
}
