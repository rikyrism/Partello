// A real-size fragment of the Partello room, built in HTML (no screenshots,
// no device frame). The stills are videos trending on YouTube today (or
// built-in trailers), credited in the footer. Names in it are sample content.

import { useEffect, useState } from 'react'
import { Link2, Mic, MicOff, Pause, SkipForward } from 'lucide-react'
import { cx, formatTime } from '../../lib/format'
import { msg, useT } from '../../i18n'
import { shortTitle, still, type GenreVideo } from './genres'
import { usePicks } from './trending'

// Built-in trailers' real lengths, for when a video's length is unknown.
const FALLBACK_SECONDS = [151, 118, 142]

/** The three hero stills and where the fake room is in each. */
function heroStills(featured: GenreVideo[]) {
  return featured.slice(0, 3).map((v, i) => {
    const length = v.duration ?? FALLBACK_SECONDS[i]!
    return { src: still(v), title: shortTitle(v.title), time: formatTime(length * (0.3 + i * 0.2)), length: formatTime(length) }
  })
}

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
  { who: 'Nita', hue: 200, body: msg('this part gets me every time'), at: '21:04' },
  { who: 'Riky', hue: 30, body: msg('pause, getting snacks'), at: '21:05' },
  { who: 'Molly', hue: 140, body: msg('paused for everyone, go'), at: '21:05' },
  { who: 'Nita', hue: 200, body: msg('back. play!'), at: '21:07' },
]

export function RoomFragment({ className }: { className?: string }) {
  const t = useT()
  const featured = usePicks()?.featured
  const stills = featured ? heroStills(featured) : []
  const upNext = featured?.[3]
  const index = useCrossfade(3)
  const current = stills[index]

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
        {stills.map((s, i) => (
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
        {current && <span className="absolute top-3 left-3 max-w-[calc(100%-1.5rem)] truncate bg-black/60 px-2 py-1 text-small text-fog">{current.title}</span>}
      </div>
      <div className="flex shrink-0 items-center gap-3 bg-night-2 px-3 py-2">
        <span className="grid size-9 place-items-center bg-night-3">
          <Pause className="size-4" />
        </span>
        <span className="tabular text-small text-haze">{current ? `${current.time} / ${current.length}` : '0:00 / 0:00'}</span>
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
            {upNext && (
              <>
                <img src={still(upNext, 'mq')} alt="" className="size-full object-cover opacity-80" />
                <span className="absolute inset-x-0 bottom-0 truncate bg-black/70 px-1.5 py-0.5 text-small">
                  {t('Up next')} · {shortTitle(upNext.title)}
                </span>
              </>
            )}
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
