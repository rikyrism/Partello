// Client-side YouTube link check for instant feedback. The server re-validates
// every URL (server/src/youtube.rs) and also checks embeddability.

const ID = /^[A-Za-z0-9_-]{11}$/
const HOSTS = new Set(['youtube.com', 'm.youtube.com', 'music.youtube.com', 'youtu.be', 'youtube-nocookie.com'])

export function parseYouTubeId(input: string): string | null {
  const raw = input.trim()
  if (!raw || raw.length > 2048) return null
  let url: URL
  try {
    url = new URL(/^https?:\/\//i.test(raw) ? raw : `https://${raw}`)
  } catch {
    return null
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return null
  const host = url.hostname.toLowerCase().replace(/^www\./, '')
  if (!HOSTS.has(host)) return null
  const parts = url.pathname.split('/').filter(Boolean)
  let id: string | null | undefined
  if (host === 'youtu.be') id = parts[0]
  else if (parts[0] === 'watch') id = url.searchParams.get('v')
  else if (parts[0] && ['embed', 'shorts', 'live', 'v'].includes(parts[0])) id = parts[1]
  return id && ID.test(id) ? id : null
}

/** The `list=` id of a regular playlist. Mixes ("RD…") are made per viewer and can't be imported. */
export function playlistIdOf(v: string): string | null {
  const id = /[?&]list=([A-Za-z0-9_-]{2,64})/.exec(v)?.[1]
  return id && !id.startsWith('RD') ? id : null
}
