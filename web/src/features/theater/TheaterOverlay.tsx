import { useState, type FormEvent } from 'react'
import { MessageSquare, MessageSquareOff, SendHorizontal } from 'lucide-react'
import { useController, useRoom } from '../../room/context'
import type { MessageEntry } from '../../room/store'
import { IconButton } from '../../components/Button'
import { CallControls } from '../call/CallStage'
import { ReactionBar } from '../reactions/Reactions'
import { cx } from '../../lib/format'
import { useT } from '../../i18n'

const SHOWN = 7

/**
 * Chat, reactions and the call over the video in theater mode, like a
 * stream overlay. Everything but the controls lets clicks reach the player.
 */
export function TheaterOverlay() {
  const t = useT()
  const [chatOn, setChatOn] = useState(true)
  const selfId = useRoom((s) => s.selfId)
  const chat = useRoom((s) => s.chat)
  const recent = chat.filter((e): e is MessageEntry => e.kind === 'message').slice(-SHOWN)

  return (
    // Bottom padding keeps YouTube's own progress bar and buttons reachable.
    <div className="pointer-events-none absolute inset-0 flex flex-col justify-end gap-2 px-3 pt-3 pb-16 sm:px-5">
      {chatOn && (
        <ol className="flex w-[min(100%,24rem)] flex-col gap-1.5 self-end" aria-label={t('Recent messages')} aria-live="polite">
          {recent.map((e, i) => (
            <li
              key={e.message.nonce ?? e.message.id}
              className={cx(
                'pop-in bg-black/60 px-3 py-1.5 text-[0.9375rem] leading-snug text-white backdrop-blur-sm',
                i < recent.length - 4 && 'opacity-60',
              )}
            >
              <span className="font-semibold text-[#9db4ff]">{e.message.participant_id === selfId ? t('You') : e.message.display_name}</span>{' '}
              <span className="break-words">{e.message.body}</span>
            </li>
          ))}
        </ol>
      )}
      <div className="pointer-events-auto flex flex-wrap items-end justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2 bg-black/60 p-1 backdrop-blur-sm">
          <ReactionBar />
          <CallControls compact />
        </div>
        <div className="flex items-end gap-2">
          {chatOn && <TheaterComposer />}
          <IconButton label={chatOn ? t('Hide chat') : t('Show chat')} onClick={() => setChatOn((v) => !v)} className="bg-black/60">
            {chatOn ? <MessageSquareOff className="size-[18px]" /> : <MessageSquare className="size-[18px]" />}
          </IconButton>
        </div>
      </div>
    </div>
  )
}

function TheaterComposer() {
  const t = useT()
  const ctrl = useController()
  const connected = useRoom((s) => s.status === 'open')
  const [text, setText] = useState('')
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (text.trim() && ctrl.sendChat(text)) setText('')
  }
  return (
    <form onSubmit={submit} className="flex w-[min(70vw,24rem)] items-center bg-black/60 backdrop-blur-sm">
      <label htmlFor="theater-chat" className="sr-only">
        {t('Message')}
      </label>
      <input
        id="theater-chat"
        value={text}
        onChange={(e) => setText(e.target.value)}
        maxLength={1000}
        autoComplete="off"
        placeholder={connected ? t('Message the room') : t('Waiting for connection')}
        className="h-11 min-w-0 flex-1 bg-transparent px-3 text-[0.9375rem] text-white placeholder:text-white/60 focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
      />
      <IconButton type="submit" label={t('Send message')} disabled={!connected || !text.trim()}>
        <SendHorizontal className="size-[18px]" />
      </IconButton>
    </form>
  )
}
