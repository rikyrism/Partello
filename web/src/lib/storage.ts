// Browser storage that never throws (private mode, blocked site data).
//   localStorage:   display name, host keys (survive restarts)
//   sessionStorage: per-tab session token (each tab is its own participant)

function safe<T>(fn: () => T, fallback: T): T {
  try {
    return fn()
  } catch {
    return fallback
  }
}

const NAME_KEY = 'partello:name'
const hostKey = (slug: string) => `partello:host:${slug}`
const sessionKey = (slug: string) => `partello:session:${slug}`
const pinKey = (slug: string) => `partello:pin:${slug}`

export interface StoredSession {
  token: string
  participantId: string
  /** The name this tab joined with; used to rejoin after the server forgets us. */
  displayName?: string
  /** Room PIN this tab entered (tab-only storage), for the same silent rejoin. */
  pin?: string
}

export const storage = {
  getName: () => safe(() => localStorage.getItem(NAME_KEY) ?? '', ''),
  setName: (name: string) => safe(() => localStorage.setItem(NAME_KEY, name), undefined),

  getHostKey: (slug: string) => safe(() => localStorage.getItem(hostKey(slug)), null),
  setHostKey: (slug: string, key: string) => safe(() => localStorage.setItem(hostKey(slug), key), undefined),

  getSession: (slug: string): StoredSession | null =>
    safe(() => {
      const raw = sessionStorage.getItem(sessionKey(slug))
      return raw ? (JSON.parse(raw) as StoredSession) : null
    }, null),
  setSession: (slug: string, s: StoredSession) =>
    safe(() => sessionStorage.setItem(sessionKey(slug), JSON.stringify(s)), undefined),
  clearSession: (slug: string) => safe(() => sessionStorage.removeItem(sessionKey(slug)), undefined),

  /**
   * The room PIN as known on this device (set by the host here, or entered to
   * join). The server keeps only a hash, so this is what invitations can show.
   */
  getRoomPin: (slug: string) => safe(() => localStorage.getItem(pinKey(slug)), null),
  setRoomPin: (slug: string, pin: string | null) =>
    safe(() => (pin ? localStorage.setItem(pinKey(slug), pin) : localStorage.removeItem(pinKey(slug))), undefined),
}
