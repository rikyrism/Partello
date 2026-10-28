import { createContext, useContext, useSyncExternalStore } from 'react'
import type { RoomController } from './controller'
import type { RoomState } from './store'

export const RoomContext = createContext<RoomController | null>(null)

export function useController(): RoomController {
  const ctrl = useContext(RoomContext)
  if (!ctrl) throw new Error('useController must be used inside a room')
  return ctrl
}

/** Subscribes to one slice of room state. Selectors must return stable references. */
export function useRoom<T>(selector: (s: RoomState) => T): T {
  const { store } = useController()
  return useSyncExternalStore(store.subscribe, () => selector(store.getState()))
}

export function useSelf() {
  const selfId = useRoom((s) => s.selfId)
  const participants = useRoom((s) => s.participants)
  return participants.find((p) => p.id === selfId) ?? null
}

/** Whether the current user may play/pause/seek/skip. */
export function useCanControl(): boolean {
  const self = useSelf()
  const mode = useRoom((s) => s.room?.settings.control_mode)
  return !!self && (self.is_host || mode === 'everyone')
}
