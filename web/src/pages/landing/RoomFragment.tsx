// A real-size fragment of the Partello room, built in HTML (no screenshots,
// no device frame). The film stills are public-domain films a room could be
// watching; credits are in the footer. Names in it are sample content.

import { useEffect, useState } from 'react'
import { Link2, Mic, MicOff, Pause, SkipForward } from 'lucide-react'
import { cx } from '../../lib/format'
import { msg, useT } from '../../i18n'
import { FEATURED, thumb } from './genres'

// Official trailer thumbnails, served by YouTube (see genres.ts).
export const STILLS = [
  { src: thumb(FEATURED.spongebob.id, 'maxres'), title: 'The SpongeBob Movie: Search for SquarePants', time: '1:12', length: '2:31' },
  { src: thumb(FEATURED.spongebobTeaser.id, 'maxres'), title: 'The SpongeBob Movie: Search for SquarePants', time: '0:48', length: '1:58' },
  { src: thumb(FEATURED.wildwood.id, 'maxres'), title: 'Wildwood', time: '1:37', length: '2:22' },
]

/** Slow crossfade between stills; pauses when the tab is hidden, static under reduced motion. */
function useCrossfade(count: number, holdMs = 5600) {
  const [index, setIndex] = useState(0)
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    let id: ReturnType<typeof setInterval> | undefined
    const start = () => {
      clearInterval(id)
      id = setInterval(() => setIndex((i) => (i + 1) % count), holdMs)
    }
    const onVis = () => (document.hidden ? clearInterval(id) : start())
    start()
    document.addEventListener('visibilitychange', onVis)
    return () => {
      clearInterval(id)
      document.removeEventListener('visibilitychange', onVis)
    }
  }, [count, holdMs])
  return index
}

const CHAT = [
  { who: 'Nita', hue: 200, body: msg('patrick is literally me'), at: '21:04' },
  { who: 'Riky', hue: 30, body: msg('pause, getting krabby patties'), at: '21:05' },
  { who: 'Molly', hue: 140, body: msg('paused for everyone, go'), at: '21:05' },
  { who: 'Nita', hue: 200, body: msg('back. play!'), at: '21:07' },
]

export function RoomFragment({ className }: { className?: string }) {
  const t = useT()
  const index = useCrossfade(STILLS.length)
  const still = STILLS[index]!

  return (
    <div data-ui className={cx('flex size-full flex-col bg-night text-fog select-none', className)} aria-hidden="true">
      <div className="flex h-12 shrink-0 items-center gap-3 px-4">
        <span className="font-medium">{t('Friday film club')}</span>
        <span className="inline-flex items-center gap-1.5 text-small text-haze">
          <span className="size-2 rounded-full bg-biro-bright" /> {t('Live')}
        </span>
        <span className="ml-auto inline-flex h-8 items-center gap-1.5 bg-biro px-3 text-small font-medium text-white">
          <Link2 className="size-3.5" /> {t('Invite')}
        </span>
      </div>

      <div className="relative aspect-video w-full shrink-0 overflow-hidden bg-black">
        {STILLS.map((s, i) => (
          <img
            key={s.src}
            src={s.src}
            alt=""
            className={cx(
              'drift absolute inset-0 size-full object-cover transition-opacity duration-[1600ms] ease-in-out',
              i === index ? 'opacity-100' : 'opacity-0',
            )}
          />
        ))}
        <span className="absolute top-3 left-3 bg-black/60 px-2 py-1 text-small text-fog">{still.title}</span>
      </div>
      <div className="flex shrink-0 items-center gap-3 bg-night-2 px-3 py-2">
        <span className="grid size-9 place-items-center bg-night-3">
          <Pause className="size-4" />
        </span>
        <span className="tabular text-small text-haze">{still.time} / {still.length}</span>
        <span className="relative h-1.5 flex-1 bg-night-4">
          <span className="absolute inset-y-0 left-0 bg-biro-bright transition-[width] duration-[1600ms]" style={{ width: `${18 + index * 27}%` }} />
        </span>
        <span className="grid size-9 place-items-center bg-night-3">
          <SkipForward className="size-4" />
        </span>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-2 bg-night p-2 md:grid-cols-[1fr_1.1fr]">
        <div className="grid min-h-0 grid-cols-2 grid-rows-2 gap-2">
          {[
            { n: 'Nita', hue: 200, speaking: true },
            { n: 'Riky', hue: 30, muted: true },
            { n: 'Molly', hue: 140 },
          ].map((p) => (
            <div key={p.n} className={cx('relative grid min-h-16 place-items-center bg-night-3', p.speaking && 'is-speaking')}>
              <span className="grid size-11 place-items-center text-base font-semibold" style={{ background: `hsl(${p.hue} 45% 32%)`, color: `hsl(${p.hue} 80% 92%)` }}>
                {p.n[0]}
              </span>
              <span className="absolute inset-x-0 bottom-0 flex items-center gap-1 bg-black/55 px-1.5 py-0.5 text-small">
                <span className="flex-1 truncate">{p.n}</span>
                {p.muted ? <MicOff className="size-3 text-alarm" /> : <Mic className="size-3" />}
              </span>
            </div>
          ))}
          <div className="relative min-h-16 overflow-hidden bg-night-3">
            <img src={thumb(FEATURED.notAlone.id, 'mq')} alt="" className="size-full object-cover opacity-80" />
            <span className="absolute inset-x-0 bottom-0 bg-black/70 px-1.5 py-0.5 text-small">{t('Up next')} · Not Alone</span>
          </div>
        </div>
        <div className="hidden min-h-0 flex-col bg-night-2 md:flex">
          <div className="flex h-10 shrink-0 bg-night text-small">
            <span className="flex flex-1 items-center justify-center bg-night-2 font-medium">{t('Chat')}</span>
            <span className="flex flex-1 items-center justify-center text-haze">{t('Queue')} 2</span>
            <span className="flex flex-1 items-center justify-center text-haze">{t('People')} 3</span>
          </div>
          <ol className="flex min-h-0 flex-1 flex-col justify-end gap-2.5 overflow-hidden p-3">
            <li className="text-small text-haze">{t('{name} paused for everyone', { name: 'Molly' })}</li>
            {CHAT.map((m, i) => (
              <li key={i} className="flex gap-2">
                <span className="grid size-7 shrink-0 place-items-center text-xs font-semibold" style={{ background: `hsl(${m.hue} 45% 32%)`, color: `hsl(${m.hue} 80% 92%)` }}>
                  {m.who[0]}
                </span>
                <span className="min-w-0">
                  <span className="text-small font-medium">{m.who}</span>{' '}
                  <span className="tabular text-small text-haze">{m.at}</span>
                  <span className="block text-small leading-snug">{t(m.body)}</span>
                </span>
              </li>
            ))}
          </ol>
          <div className="m-2 flex h-9 shrink-0 items-center bg-night-3 px-3 text-small text-haze">{t('Message the room')}</div>
        </div>
      </div>
    </div>
  )
}
