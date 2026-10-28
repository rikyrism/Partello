import { SkipForward } from 'lucide-react'
import { useController, useRoom } from '../../room/context'
import { cx } from '../../lib/format'
import { useT } from '../../i18n'

/** Majority of the people connected right now (mirrors the server rule). */
export function votesNeeded(connected: number): number {
  return Math.floor(connected / 2) + 1
}

/** For people who can't skip: ask the room. The video changes once most agree. */
export function SkipVoteButton() {
  const t = useT()
  const ctrl = useController()
  const itemId = useRoom((s) => (s.playback?.status === 'playing' || s.playback?.status === 'paused' ? (s.playback.item?.id ?? null) : null))
  const voters = useRoom((s) => s.skipVotes)
  const participants = useRoom((s) => s.participants)
  const selfId = useRoom((s) => s.selfId)

  const here = participants.filter((p) => p.connected)
  const count = voters.filter((v) => here.some((p) => p.id === v)).length
  const needed = votesNeeded(here.length)
  const mine = !!selfId && voters.includes(selfId)

  return (
    <button
      type="button"
      disabled={!itemId}
      aria-pressed={mine}
      onClick={() => itemId && ctrl.skipVote(itemId, !mine)}
      title={t('The video is skipped when most people here vote for it')}
      className={cx(
        'inline-flex h-11 shrink-0 items-center gap-1.5 px-2.5 text-small font-medium transition-colors focus-visible:outline-2 focus-visible:outline-biro-bright disabled:opacity-40',
        mine ? 'bg-biro text-white' : 'text-haze hover:bg-night-3 hover:text-fog',
      )}
    >
      <SkipForward className="size-4" aria-hidden="true" />
      <span>{mine ? t('Voted to skip') : t('Vote to skip')}</span>
      {count > 0 && <span className="tabular">{t('{n} of {max}', { n: count, max: needed })}</span>}
    </button>
  )
}
