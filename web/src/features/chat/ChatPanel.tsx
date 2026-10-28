import { memo, useEffect, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import { ArrowDown, RotateCcw, SendHorizontal } from 'lucide-react'
import { Avatar } from '../../components/Avatar'
import { IconButton } from '../../components/Button'
import { useController, useRoom } from '../../room/context'
import type { ChatEntry, MessageEntry } from '../../room/store'
import { cx, formatClock } from '../../lib/format'
import { useT } from '../../i18n'

const GROUP_WINDOW_MS = 3 * 60_000

export function ChatPanel() {
  const t = useT()
  const chat = useRoom((s) => s.chat)
  const synced = useRoom((s) => s.synced)
  const selfId = useRoom((s) => s.selfId)
  const listRef = useRef<HTMLDivElement>(null)
  const stickRef = useRef(true)
  const [showJump, setShowJump] = useState(false)

  const onScroll = () => {
    const el = listRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48
    stickRef.current = atBottom
    if (atBottom) setShowJump(false)
  }

  // Stay pinned to the newest message unless the reader scrolled up.
  useLayoutEffect(() => {
    const el = listRef.current
    if (!el) return
    if (stickRef.current) el.scrollTop = el.scrollHeight
    else setShowJump(true)
  }, [chat])

  const jump = () => {
    const el = listRef.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
    stickRef.current = true
    setShowJump(false)
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="relative min-h-0 flex-1">
        <div
          ref={listRef}
          onScroll={onScroll}
          className="scrollbar-thin h-full overflow-y-auto px-3 py-3"
          role="log"
          aria-live="polite"
          aria-label={t('Chat messages')}
        >
          {!synced ? (
            <ChatSkeleton />
          ) : chat.length === 0 ? (
            <div className="flex h-full flex-col justify-end pb-2">
              <p className="text-lead font-medium text-fog">{t('No messages yet.')}</p>
              <p className="text-haze">{t('Say hello. Everyone in the room sees it instantly.')}</p>
            </div>
          ) : (
            <ol className="flex flex-col">
              {chat.map((entry, i) => (
                <ChatRow key={entry.kind === 'system' ? entry.id : entry.message.nonce ?? entry.message.id} entry={entry} prev={chat[i - 1]} selfId={selfId} />
              ))}
            </ol>
          )}
        </div>
        {showJump && (
          <button
            onClick={jump}
            className="pop-in absolute bottom-3 left-1/2 inline-flex -translate-x-1/2 items-center gap-1.5 bg-fog px-3 py-1.5 text-small font-medium text-night"
          >
            <ArrowDown className="size-3.5" aria-hidden="true" /> {t('New messages')}
          </button>
        )}
      </div>
      <Composer />
    </div>
  )
}

function ChatSkeleton() {
  const t = useT()
  return (
    <div className="flex flex-col gap-4" aria-label={t('Loading messages')}>
      {[60, 85, 40].map((w) => (
        <div key={w} className="flex gap-2.5">
          <div className="size-9 animate-pulse bg-night-3" />
          <div className="flex flex-1 flex-col gap-2 pt-1">
            <div className="h-3 w-20 animate-pulse bg-night-3" />
            <div className="h-3 animate-pulse bg-night-3" style={{ width: `${w}%` }} />
          </div>
        </div>
      ))}
    </div>
  )
}

const ChatRow = memo(function ChatRow({ entry, prev, selfId }: { entry: ChatEntry; prev?: ChatEntry; selfId: string | null }) {
  const t = useT()
  if (entry.kind === 'system') {
    return <li className="py-1.5 text-small text-haze">{t(entry.text, entry.vars)}</li>
  }
  const m = entry.message
  const grouped =
    prev?.kind === 'message' &&
    prev.message.participant_id === m.participant_id &&
    m.created_at - prev.message.created_at < GROUP_WINDOW_MS
  const isSelf = m.participant_id === selfId

  return (
    <li className={cx('flex gap-2.5', grouped ? 'pt-0.5' : 'pt-3', entry.pending && 'opacity-60')}>
      <div className="w-9 shrink-0">{!grouped && <Avatar id={m.participant_id} name={m.display_name} />}</div>
      <div className="min-w-0 flex-1">
        {!grouped && (
          <div className="flex items-baseline gap-2">
            <span className="truncate font-medium text-fog">{isSelf ? t('You') : m.display_name}</span>
            <time className="tabular shrink-0 text-small text-haze" dateTime={new Date(m.created_at).toISOString()}>
              {formatClock(m.created_at)}
            </time>
          </div>
        )}
        <p className="whitespace-pre-wrap break-words text-[0.9375rem] leading-relaxed text-fog">{m.body}</p>
        {entry.failed && <RetryNote entry={entry} />}
      </div>
    </li>
  )
})

function RetryNote({ entry }: { entry: MessageEntry }) {
  const t = useT()
  const ctrl = useController()
  return (
    <button
      className="mt-0.5 inline-flex items-center gap-1 text-small text-alarm hover:underline"
      onClick={() => ctrl.sendChat(entry.message.body)}
    >
      <RotateCcw className="size-3" aria-hidden="true" /> {t('Not sent. Send again')}
    </button>
  )
}

function Composer() {
  const t = useT()
  const ctrl = useController()
  const status = useRoom((s) => s.status)
  const max = useRoom((s) => s.limits?.chat_max_chars ?? 500)
  const [text, setText] = useState('')
  const ref = useRef<HTMLTextAreaElement>(null)
  const connected = status === 'open'
  const length = [...text.trim()].length
  const tooLong = length > max

  // Grow with content up to ~5 lines.
  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 140)}px`
  }, [text])

  const submit = (e?: FormEvent) => {
    e?.preventDefault()
    if (!connected || length === 0 || tooLong) return
    if (ctrl.sendChat(text)) setText('')
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault()
      submit()
    }
  }

  return (
    <form onSubmit={submit} className="bg-night-2 p-2">
      {!connected && (
        <p className="px-1 pb-2 text-small text-haze" role="status">
          {status === 'closed' ? t('Disconnected from the room.') : t('Reconnecting… messages will send once you are back.')}
        </p>
      )}
      <div className="flex items-end gap-2">
        <label htmlFor="chat-input" className="sr-only">
          {t('Message')}
        </label>
        <textarea
          id="chat-input"
          ref={ref}
          rows={1}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={connected ? t('Message the room') : t('Waiting for connection')}
          maxLength={max * 2}
          className="scrollbar-thin min-h-11 flex-1 resize-none bg-night-3 px-3 py-2.5 text-[0.9375rem] text-fog placeholder:text-haze focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
          aria-describedby={length > max * 0.8 ? 'chat-count' : undefined}
        />
        <IconButton type="submit" label={t('Send message')} disabled={!connected || length === 0 || tooLong} active={length > 0 && !tooLong}>
          <SendHorizontal className="size-[18px]" />
        </IconButton>
      </div>
      {length > max * 0.8 && (
        <p id="chat-count" className={cx('tabular px-1 pt-1 text-right text-small', tooLong ? 'text-alarm' : 'text-haze')}>
          {length}/{max}
        </p>
      )}
    </form>
  )
}
