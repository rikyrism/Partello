import { useMutation } from '@tanstack/react-query'
import { useNavigate } from 'react-router'
import { api } from '../../lib/api'
import { storage } from '../../lib/storage'

/** Creates a room, joins it as host, and opens it. */
export function useCreateRoom() {
  const navigate = useNavigate()
  return useMutation({
    mutationFn: async ({ name, roomName, videoUrl }: { name: string; roomName?: string; videoUrl?: string }) => {
      const created = await api.createRoom(name, roomName, videoUrl)
      const slug = created.room.slug
      storage.setName(name)
      storage.setHostKey(slug, created.host_key)
      const joined = await api.joinRoom(slug, name, created.host_key)
      storage.setSession(slug, { token: joined.session_token, participantId: joined.participant_id, displayName: name })
      return slug
    },
    onSuccess: (slug) => navigate(`/r/${slug}`),
  })
}

/** Accepts a full invite link or a bare 12-character room code. */
export function roomSlugFrom(input: string): string | null {
  const raw = input.trim()
  const fromPath = /\/r\/([A-Za-z0-9]{12})(?:[/?#]|$)/.exec(raw)?.[1]
  if (fromPath) return fromPath
  return /^[A-Za-z0-9]{12}$/.test(raw) ? raw : null
}
