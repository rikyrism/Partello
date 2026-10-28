// "Who is talking" detection, done locally from audio levels so nothing extra
// travels over the network.

import { useEffect, useState } from 'react'

let ctx: AudioContext | null = null

/** Create/resume inside a user gesture (joining the call). */
export function audioContext(): AudioContext | null {
  if (typeof AudioContext === 'undefined') return null
  ctx ??= new AudioContext()
  if (ctx.state === 'suspended') void ctx.resume()
  return ctx
}

/**
 * True while anyone in `streams` is talking (with a short hold), using one
 * analyser per stream on a single timer. Drives video ducking.
 */
export function useAnySpeaking(streams: MediaStream[], enabled: boolean, holdMs = 900): boolean {
  const [speaking, setSpeaking] = useState(false)
  const key = streams.map((s) => s.id).join(',')

  useEffect(() => {
    const ac = ctx
    if (!enabled || !ac || streams.length === 0) return
    const probes = streams
      .map((s) => s.getAudioTracks()[0])
      .filter((t): t is MediaStreamTrack => !!t)
      .map((track) => {
        const source = ac.createMediaStreamSource(new MediaStream([track]))
        const analyser = ac.createAnalyser()
        analyser.fftSize = 512
        source.connect(analyser)
        return { track, source, analyser, data: new Float32Array(analyser.fftSize) }
      })
    let lastLoud = 0
    let current = false
    const id = setInterval(() => {
      const now = performance.now()
      for (const p of probes) {
        if (!p.track.enabled) continue
        p.analyser.getFloatTimeDomainData(p.data)
        let sum = 0
        for (const v of p.data) sum += v * v
        if (Math.sqrt(sum / p.data.length) > THRESHOLD) lastLoud = now
      }
      const next = now - lastLoud < holdMs
      if (next !== current) {
        current = next
        setSpeaking(next)
      }
    }, 120)
    return () => {
      clearInterval(id)
      probes.forEach((p) => p.source.disconnect())
      setSpeaking(false)
    }
    // `key` captures the identity of the stream list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled, holdMs])

  return enabled && speaking
}

const THRESHOLD = 0.035
const HOLD_MS = 350

export function useSpeaking(stream: MediaStream | null | undefined, enabled = true): boolean {
  const [speaking, setSpeaking] = useState(false)

  useEffect(() => {
    const track = stream?.getAudioTracks()[0]
    const ac = ctx
    if (!enabled || !stream || !track || !ac) return
    const source = ac.createMediaStreamSource(new MediaStream([track]))
    const analyser = ac.createAnalyser()
    analyser.fftSize = 512
    source.connect(analyser)
    const data = new Float32Array(analyser.fftSize)
    let lastLoud = 0
    let current = false
    const id = setInterval(() => {
      analyser.getFloatTimeDomainData(data)
      let sum = 0
      for (const v of data) sum += v * v
      const rms = Math.sqrt(sum / data.length)
      const now = performance.now()
      if (rms > THRESHOLD && track.enabled) lastLoud = now
      const next = now - lastLoud < HOLD_MS
      if (next !== current) {
        current = next
        setSpeaking(next)
      }
    }, 100)
    return () => {
      clearInterval(id)
      source.disconnect()
      setSpeaking(false)
    }
  }, [stream, enabled])

  return enabled && speaking
}
