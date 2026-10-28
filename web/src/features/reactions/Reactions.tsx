import { useEffect, useRef, useState } from 'react'
import { useController, useRoom } from '../../room/context'
import { REACTIONS } from '../../realtime/protocol'
import { cx } from '../../lib/format'
import { useT } from '../../i18n'

interface Floating {
  key: number
  emoji: string
  name: string
  /** Horizontal start, percent of the player width. */
  x: number
  /** Small sideways sway so a burst does not stack in one column. */
  sway: number
}

const SHOW_MS = 2600
const MAX_ON_SCREEN = 24

/** Emoji drifting up over the video. Never takes clicks away from the player. */
export function ReactionOverlay() {
  const ctrl = useController()
  const [items, setItems] = useState<Floating[]>([])
  const seq = useRef(0)

  useEffect(
    () =>
      ctrl.onReaction((participantId, emoji) => {
        const state = ctrl.store.getState()
        const person = state.participants.find((p) => p.id === participantId)
        const key = ++seq.current
        const item: Floating = {
          key,
          emoji,
          name: participantId === state.selfId ? '' : (person?.display_name ?? ''),
          x: 6 + Math.random() * 80,
          sway: Math.round((Math.random() - 0.5) * 60),
        }
        setItems((list) => [...list.slice(-(MAX_ON_SCREEN - 1)), item])
        setTimeout(() => setItems((list) => list.filter((i) => i.key !== key)), SHOW_MS)
      }),
    [ctrl],
  )

  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
      {items.map((i) => (
        <span
          key={i.key}
          className="reaction-float absolute bottom-3 flex flex-col items-center"
          style={{ left: `${i.x}%`, ['--sway' as string]: `${i.sway}px` }}
        >
          <span className="text-[clamp(1.75rem,4vw,2.75rem)] leading-none">{i.emoji}</span>
          {i.name && <span className="max-w-28 truncate bg-black/60 px-1.5 text-xs text-white">{i.name}</span>}
        </span>
      ))}
    </div>
  )
}

/** One tap per emoji; everyone in the room sees it float over the video. */
export function ReactionBar({ className }: { className?: string }) {
  const t = useT()
  const ctrl = useController()
  const connected = useRoom((s) => s.status === 'open')
  return (
    <div role="group" aria-label={t('React')} className={cx('flex items-center', className)}>
      {REACTIONS.map((emoji) => (
        <button
          key={emoji}
          type="button"
          disabled={!connected}
          onClick={() => ctrl.react(emoji)}
          aria-label={t('React with {emoji}', { emoji })}
          className="grid size-9 place-items-center text-lg transition-transform hover:scale-125 active:scale-95 focus-visible:outline-2 focus-visible:outline-biro-bright disabled:opacity-40"
        >
          {emoji}
        </button>
      ))}
    </div>
  )
}
