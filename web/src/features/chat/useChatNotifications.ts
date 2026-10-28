import { useEffect, useRef } from 'react'
import { useController } from '../../room/context'
import { storage } from '../../lib/storage'
import { audioContext } from '../call/speaking'
import { mentions } from './mentions'
import { playChatSound } from './sounds'
import { notifyLocally } from '../../lib/pwa'

/**
 * Sounds for chat: always when someone mentions you or replies to you, and
 * for other messages only when you can't see the chat (other tab, other panel).
 */
export function useChatNotifications(chatVisible: boolean): void {
  const ctrl = useController()
  const visibleRef = useRef(chatVisible)
  useEffect(() => {
    visibleRef.current = chatVisible
  }, [chatVisible])

  // Browsers only allow sound after the first click or key press on the page.
  useEffect(() => {
    const unlock = () => audioContext()
    window.addEventListener('pointerdown', unlock, { once: true })
    window.addEventListener('keydown', unlock, { once: true })
    return () => {
      window.removeEventListener('pointerdown', unlock)
      window.removeEventListener('keydown', unlock)
    }
  }, [])

  useEffect(
    () =>
      ctrl.onMessage((message) => {
        if (!storage.getChatSounds()) return
        const state = ctrl.store.getState()
        const self = state.participants.find((p) => p.id === state.selfId)
        const forMe = !!self && (mentions(message.body, self.display_name) || message.reply_to?.participant_id === self.id)
        if (forMe) {
          playChatSound('mention')
          // In another tab or app: a system notification, if allowed earlier.
          if (document.hidden) void notifyLocally(state.room?.name ?? 'Partello', `${message.display_name}: ${message.body}`, `mention-${message.id}`)
        } else if (document.hidden || !visibleRef.current) playChatSound('message')
      }),
    [ctrl],
  )
}
