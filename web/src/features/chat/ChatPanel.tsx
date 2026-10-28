import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import { ArrowDown, Bell, BellOff, CornerUpLeft, RotateCcw, SendHorizontal, X } from 'lucide-react'
import { Avatar } from '../../components/Avatar'
import { IconButton } from '../../components/Button'
import { useCanControl, useController, useRoom } from '../../room/context'
import type { ChatEntry, MessageEntry } from '../../room/store'
import type { ChatMessage, Participant } from '../../realtime/protocol'
import { cx, formatClock, formatTime } from '../../lib/format'
import { storage } from '../../lib/storage'
import { useT } from '../../i18n'
import { mentionQuery, mentions, splitMentions } from './mentions'
import { splitTimestamps } from './timestamps'

const GROUP_WINDOW_MS = 3 * 60_000

export function ChatPanel() {
  const t = useT()
  const chat = useRoom((s) => s.chat)
  const synced = useRoom((s) => s.synced)
  const selfId = useRoom((s) => s.selfId)
  const participants = useRoom((s) => s.participants)
  const [replyTo, setReplyTo] = useState<ChatMessage | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const selfName = participants.find((p) => p.id === selfId)?.display_name ?? ''
  // Names seen in this chat (people who left included) can be mentioned.
  const namesKey = useMemo(() => {
    const names = new Set(participants.map((p) => p.display_name))
    for (const e of chat) if (e.kind === 'message') names.add(e.message.display_name)
    return [...names].join('\n')
  }, [participants, chat])
  const names = useMemo(() => (namesKey ? namesKey.split('\n') : []), [namesKey])

  const reply = useCallback((m: ChatMessage) => {
    setReplyTo(m)
    document.getElementById('chat-input')?.focus()
  }, [])
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
                <ChatRow
                  key={entry.kind === 'system' ? entry.id : entry.message.nonce ?? entry.message.id}
                  entry={entry}
                  prev={chat[i - 1]}
                  selfId={selfId}
                  selfName={selfName}
                  names={names}
                  onReply={reply}
                />
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
      <Composer replyTo={replyTo} onCancelReply={() => setReplyTo(null)} participants={participants} selfId={selfId} />
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

const ChatRow = memo(function ChatRow({
  entry,
  prev,
  selfId,
  selfName,
  names,
  onReply,
}: {
  entry: ChatEntry
  prev?: ChatEntry
  selfId: string | null
  selfName: string
  names: string[]
  onReply: (m: ChatMessage) => void
}) {
  const t = useT()
  if (entry.kind === 'system') {
    return <li className="py-1.5 text-small text-haze">{t(entry.text, entry.vars)}</li>
  }
  const m = entry.message
  const grouped =
    !m.reply_to &&
    prev?.kind === 'message' &&
    prev.message.participant_id === m.participant_id &&
    m.created_at - prev.message.created_at < GROUP_WINDOW_MS
  const isSelf = m.participant_id === selfId
  const mentioned = !isSelf && (mentions(m.body, selfName) || (m.reply_to?.participant_id === selfId && !!selfId))
  const sent = !entry.pending && !entry.failed

  return (
    <li
      id={sent ? `msg-${m.id}` : undefined}
      className={cx(
        'group/msg relative flex gap-2.5',
        grouped ? 'pt-0.5' : 'pt-3',
        entry.pending && 'opacity-60',
        mentioned && '-mx-3 bg-biro/15 px-3 pb-1 shadow-[inset_3px_0_0_var(--color-biro-bright)]',
      )}
    >
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
        {m.reply_to && <Quote reply={m.reply_to} selfId={selfId} />}
        <p className="whitespace-pre-wrap break-words text-[0.9375rem] leading-relaxed text-fog">
          {splitMentions(m.body, names).map((seg, i) =>
            seg.mention ? (
              <span key={i} className={cx('font-medium', seg.mention.toLowerCase() === selfName.toLowerCase() ? 'text-white' : 'text-biro-bright')}>
                {seg.text}
              </span>
            ) : (
              <WithTimestamps key={i} text={seg.text} />
            ),
          )}
        </p>
        {entry.failed && <RetryNote entry={entry} />}
      </div>
      {sent && (
        <button
          type="button"
          onClick={() => onReply(m)}
          aria-label={t('Reply to {name}', { name: m.display_name })}
          className="absolute top-1 right-0 grid size-8 place-items-center bg-night-3 text-haze opacity-100 hover:text-fog focus-visible:outline-2 focus-visible:outline-biro-bright sm:opacity-0 sm:group-hover/msg:opacity-100 sm:focus-visible:opacity-100"
        >
          <CornerUpLeft className="size-4" aria-hidden="true" />
        </button>
      )}
    </li>
  )
})

/** Times like 12:45 jump the room there (for people who control playback). */
function WithTimestamps({ text }: { text: string }) {
  const parts = splitTimestamps(text)
  if (parts.length === 1 && parts[0]!.seconds === undefined) return <>{text}</>
  return (
    <>
      {parts.map((p, i) => (p.seconds === undefined ? p.text : <TimeLink key={i} text={p.text} seconds={p.seconds} />))}
    </>
  )
}

function TimeLink({ text, seconds }: { text: string; seconds: number }) {
  const t = useT()
  const ctrl = useController()
  const canControl = useCanControl()
  const hasVideo = useRoom((s) => s.playback?.status === 'playing' || s.playback?.status === 'paused')
  const duration = useRoom((s) => s.playback?.duration ?? null)
  if (!hasVideo || (duration != null && seconds > duration)) return <>{text}</>
  if (!canControl) {
    return (
      <span className="font-medium text-biro-bright" title={t('Only the host can jump to {time}', { time: formatTime(seconds) })}>
        {text}
      </span>
    )
  }
  return (
    <button
      type="button"
      onClick={() => ctrl.control('seek', seconds)}
      className="font-medium text-biro-bright underline decoration-biro-bright/50 underline-offset-2 hover:decoration-biro-bright"
      aria-label={t('Jump everyone to {time}', { time: formatTime(seconds) })}
    >
      {text}
    </button>
  )
}

function Quote({ reply, selfId }: { reply: NonNullable<ChatMessage['reply_to']>; selfId: string | null }) {
  const t = useT()
  const jump = () => {
    const el = document.getElementById(`msg-${reply.id}`)
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' })
  }
  return (
    <button
      type="button"
      onClick={jump}
      className="my-1 flex w-full max-w-full flex-col items-start bg-night-3 px-2.5 py-1.5 text-left text-small shadow-[inset_2px_0_0_var(--color-haze)] hover:bg-night-4"
    >
      <span className="font-medium text-fog">{reply.participant_id === selfId ? t('You') : reply.display_name}</span>
      <span className="line-clamp-2 break-words text-haze">{reply.body}</span>
    </button>
  )
}

function RetryNote({ entry }: { entry: MessageEntry }) {
  const t = useT()
  const ctrl = useController()
  const original = entry.message.reply_to
  return (
    <button
      className="mt-0.5 inline-flex items-center gap-1 text-small text-alarm hover:underline"
      onClick={() => ctrl.sendChat(entry.message.body, original)}
    >
      <RotateCcw className="size-3" aria-hidden="true" /> {t('Not sent. Send again')}
    </button>
  )
}

function Composer({
  replyTo,
  onCancelReply,
  participants,
  selfId,
}: {
  replyTo: ChatMessage | null
  onCancelReply: () => void
  participants: Participant[]
  selfId: string | null
}) {
  const t = useT()
  const ctrl = useController()
  const status = useRoom((s) => s.status)
  const max = useRoom((s) => s.limits?.chat_max_chars ?? 500)
  const [text, setText] = useState('')
  const [caret, setCaret] = useState(0)
  const [pick, setPick] = useState(0)
  const [sounds, setSounds] = useState(() => storage.getChatSounds())
  const ref = useRef<HTMLTextAreaElement>(null)
  const connected = status === 'open'
  const length = [...text.trim()].length
  const tooLong = length > max

  // "@ri" at the caret suggests people whose name starts with it.
  const query = mentionQuery(text.slice(0, caret))
  const suggestions =
    query === null
      ? []
      : participants
          .filter((p) => p.id !== selfId && p.display_name.toLowerCase().startsWith(query.toLowerCase()))
          .slice(0, 5)
  const picking = suggestions.length > 0
  const active = Math.min(pick, Math.max(0, suggestions.length - 1))

  // Grow with content up to ~5 lines.
  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 140)}px`
  }, [text])

  const complete = (p: Participant) => {
    const before = text.slice(0, caret).replace(/@[^\s@]*$/, `@${p.display_name} `)
    const next = before + text.slice(caret)
    setText(next)
    setCaret(before.length)
    setPick(0)
    requestAnimationFrame(() => {
      ref.current?.focus()
      ref.current?.setSelectionRange(before.length, before.length)
    })
  }

  const submit = (e?: FormEvent) => {
    e?.preventDefault()
    if (!connected || length === 0 || tooLong) return
    if (ctrl.sendChat(text, replyTo ?? undefined)) {
      setText('')
      setCaret(0)
      onCancelReply()
    }
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (picking) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const step = e.key === 'ArrowDown' ? 1 : suggestions.length - 1
        setPick((active + step) % suggestions.length)
        return
      }
      if ((e.key === 'Enter' || e.key === 'Tab') && !e.nativeEvent.isComposing) {
        e.preventDefault()
        complete(suggestions[active]!)
        return
      }
    }
    if (e.key === 'Escape' && replyTo) {
      e.preventDefault()
      onCancelReply()
      return
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault()
      submit()
    }
  }

  const toggleSounds = () => {
    storage.setChatSounds(!sounds)
    setSounds(!sounds)
  }

  return (
    <form onSubmit={submit} className="relative bg-night-2 p-2">
      {!connected && (
        <p className="px-1 pb-2 text-small text-haze" role="status">
          {status === 'closed' ? t('Disconnected from the room.') : t('Reconnecting… messages will send once you are back.')}
        </p>
      )}
      {replyTo && (
        <div className="mb-2 flex items-start gap-2 bg-night-3 py-1.5 pr-1 pl-2.5 shadow-[inset_2px_0_0_var(--color-biro-bright)]">
          <div className="min-w-0 flex-1 text-small">
            <p className="font-medium text-fog">{t('Replying to {name}', { name: replyTo.display_name })}</p>
            <p className="truncate text-haze">{replyTo.body}</p>
          </div>
          <IconButton size="sm" label={t('Cancel reply')} onClick={onCancelReply}>
            <X className="size-4" />
          </IconButton>
        </div>
      )}
      {picking && (
        <ul
          id="mention-list"
          role="listbox"
          aria-label={t('Mention someone')}
          className="pop-in absolute inset-x-2 bottom-full z-10 mb-1 bg-night-3 py-1 shadow-[0_-12px_32px_rgba(0,0,0,0.45)]"
        >
          {suggestions.map((p, i) => (
            <li
              key={p.id}
              id={`mention-${p.id}`}
              role="option"
              aria-selected={i === active}
              onMouseDown={(e) => {
                e.preventDefault()
                complete(p)
              }}
              onMouseEnter={() => setPick(i)}
              className={cx('flex cursor-pointer items-center gap-2 px-2.5 py-1.5', i === active && 'bg-night-4')}
            >
              <Avatar id={p.id} name={p.display_name} size="sm" />
              <span className="truncate text-[0.9375rem]">{p.display_name}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-end gap-2">
        <IconButton
          label={sounds ? t('Mute chat sounds') : t('Turn chat sounds on')}
          active={false}
          onClick={toggleSounds}
          aria-pressed={!sounds}
        >
          {sounds ? <Bell className="size-[18px]" /> : <BellOff className="size-[18px]" />}
        </IconButton>
        <label htmlFor="chat-input" className="sr-only">
          {t('Message')}
        </label>
        <textarea
          id="chat-input"
          ref={ref}
          rows={1}
          value={text}
          onChange={(e) => {
            setText(e.target.value)
            setCaret(e.target.selectionStart)
            setPick(0)
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onKeyDown={onKeyDown}
          placeholder={connected ? t('Message the room') : t('Waiting for connection')}
          maxLength={max * 2}
          role={picking ? 'combobox' : undefined}
          aria-expanded={picking ? true : undefined}
          aria-controls={picking ? 'mention-list' : undefined}
          aria-activedescendant={picking ? `mention-${suggestions[active]!.id}` : undefined}
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
