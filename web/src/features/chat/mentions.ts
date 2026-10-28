// @mentions in chat: plain text, so no markup ever reaches the server.

export type Segment = { text: string; mention?: string }

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** A mention ends at the end of the text, whitespace or punctuation. */
const END = String.raw`(?=$|[\s.,!?;:)\]}"'])`

/** True when `body` mentions `name` (case-insensitive). */
export function mentions(body: string, name: string): boolean {
  if (!name.trim()) return false
  return new RegExp(`(^|\\s)@${escape(name)}${END}`, 'i').test(body)
}

/** Splits a message into text and mentions of the given names (longest first, so "Ana Maria" beats "Ana"). */
export function splitMentions(body: string, names: string[]): Segment[] {
  const usable = [...new Set(names.filter((n) => n.trim()))].sort((a, b) => b.length - a.length)
  if (usable.length === 0 || !body.includes('@')) return [{ text: body }]
  const re = new RegExp(`(^|\\s)@(${usable.map(escape).join('|')})${END}`, 'gi')
  const out: Segment[] = []
  let last = 0
  for (const m of body.matchAll(re)) {
    const start = m.index + m[1]!.length
    if (start > last) out.push({ text: body.slice(last, start) })
    const canonical = usable.find((n) => n.toLowerCase() === m[2]!.toLowerCase()) ?? m[2]!
    out.push({ text: `@${m[2]}`, mention: canonical })
    last = start + 1 + m[2]!.length
  }
  if (last < body.length) out.push({ text: body.slice(last) })
  return out
}

/** The "@partial" being typed at the end of `beforeCaret`, if any. */
export function mentionQuery(beforeCaret: string): string | null {
  const m = /(?:^|\s)@([^\s@]{0,32})$/.exec(beforeCaret)
  return m ? m[1]! : null
}
