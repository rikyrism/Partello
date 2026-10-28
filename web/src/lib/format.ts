export function formatTime(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) totalSeconds = 0
  const s = Math.floor(totalSeconds)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = String(s % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`
}

const clock = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' })

export function formatClock(ms: number): string {
  return clock.format(new Date(ms))
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  const first = parts[0]?.[0] ?? '?'
  const second = parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? '') : ''
  return (first + second).toUpperCase()
}

/** Stable hue per participant so avatars stay recognisable. */
export function hueFor(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360
  return h
}

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(' ')
}
