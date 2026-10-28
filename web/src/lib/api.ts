// REST client for the Partello backend (same origin; Vite proxies /api in dev).

import type { RoomInfo, RoomSettings } from '../realtime/protocol'
import { translate } from '../i18n'

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message)
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
      ...init,
      headers: { 'Content-Type': 'application/json', ...init?.headers },
    })
  } catch (err) {
    if (init?.signal?.aborted) throw err
    throw new ApiError(0, 'network', translate("Can't reach Partello. Check your connection and try again."))
  }
  const body = await res.json().catch(() => null)
  if (!res.ok) {
    const err = body?.error
    // Server messages are English; show the Indonesian version when one exists.
    const message = err?.message ? translate(err.message) : translate('Request failed ({status})', { status: res.status })
    throw new ApiError(res.status, err?.code ?? 'unknown', message)
  }
  return body as T
}

export interface CreateRoomResponse {
  room: RoomInfo
  host_key: string
  invite_url: string
}

export interface RoomSummary {
  room: RoomInfo
  participant_count: number
  is_full: boolean
}

export interface JoinResponse {
  participant_id: string
  session_token: string
  is_creator: boolean
  room: RoomInfo
}

export interface SearchResult {
  video_id: string
  title: string
  channel: string
  thumbnail_url: string
  duration: number | null
  is_live: boolean
}

export interface CreateKeyResponse {
  id: string
  label: string
  api_key: string
}

export const api = {
  createRoom: (displayName: string, name?: string, videoUrl?: string) =>
    request<CreateRoomResponse>('/api/rooms', {
      method: 'POST',
      body: JSON.stringify({ display_name: displayName, name: name || undefined, video_url: videoUrl }),
    }),

  getRoom: (slug: string) => request<RoomSummary>(`/api/rooms/${encodeURIComponent(slug)}`),

  joinRoom: (slug: string, displayName: string, hostKey?: string | null, pin?: string) =>
    request<JoinResponse>(`/api/rooms/${encodeURIComponent(slug)}/join`, {
      method: 'POST',
      body: JSON.stringify({ display_name: displayName, host_key: hostKey ?? undefined, pin: pin || undefined }),
    }),

  updateSettings: (slug: string, token: string, patch: Partial<RoomSettings> & { name?: string; pin?: string }) =>
    request<RoomInfo>(`/api/rooms/${encodeURIComponent(slug)}/settings`, {
      method: 'PATCH',
      headers: { Authorization: `Bearer ${token}` },
      body: JSON.stringify(patch),
    }),

  pushKey: () => request<{ public_key: string }>('/api/push/key'),

  subscribePush: (slug: string, token: string, subscription: PushSubscriptionJSON, lang: string) =>
    request<null>(`/api/rooms/${encodeURIComponent(slug)}/push`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
      body: JSON.stringify({ subscription, lang }),
    }),

  unsubscribePush: (slug: string, token: string, endpoint: string) =>
    request<null>(`/api/rooms/${encodeURIComponent(slug)}/push`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
      body: JSON.stringify({ endpoint }),
    }),

  search: (slug: string, token: string, q: string, signal?: AbortSignal) =>
    request<{ results: SearchResult[] }>(`/api/rooms/${encodeURIComponent(slug)}/search?q=${encodeURIComponent(q)}`, {
      headers: { Authorization: `Bearer ${token}` },
      signal,
    }),

  createApiKey: (label: string) =>
    request<CreateKeyResponse>('/api/v1/keys', { method: 'POST', body: JSON.stringify({ label }) }),
}
