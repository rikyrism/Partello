// "look at 12:45" → 12:45 becomes a link that jumps the room to that second.

export type TimeSegment = { text: string; seconds?: number }

/** m:ss or h:mm:ss, standing alone: not inside a longer number, time or word ("12:45:99", "1:2", "v1.2:30"). */
const TIME = /(?<![\w.:])(?:(\d{1,2}):)?(\d{1,2}):([0-5]\d)(?![\w:])/g

export function splitTimestamps(text: string): TimeSegment[] {
  if (!text.includes(':')) return [{ text }]
  const out: TimeSegment[] = []
  let last = 0
  for (const m of text.matchAll(TIME)) {
    const [whole, h, mm, ss] = m
    // With hours, minutes must be two digits and below 60 (1:05:30, not 1:5:30).
    if (h !== undefined && (mm!.length !== 2 || Number(mm) > 59)) continue
    const seconds = Number(h ?? 0) * 3600 + Number(mm) * 60 + Number(ss)
    if (m.index > last) out.push({ text: text.slice(last, m.index) })
    out.push({ text: whole, seconds })
    last = m.index + whole.length
  }
  if (last < text.length) out.push({ text: text.slice(last) })
  return out.length ? out : [{ text }]
}
