// "Pick a genre": each card wears the thumbnail of a video trending on
// YouTube today (or a built-in trailer when the server has no chart);
// opening it lists the picks, and "Watch together" opens a room that is
// already playing the chosen video.

import { useState, type FormEvent } from 'react'
import { ArrowRight, ChevronDown, Play } from 'lucide-react'
import { useT } from '../../i18n'
import { ApiError } from '../../lib/api'
import { cx } from '../../lib/format'
import { storage } from '../../lib/storage'
import { Button } from '../../components/Button'
import { still, thumb, type GenreVideo } from './genres'
import { usePicks } from './trending'
import { useCreateRoom } from './useCreateRoom'

export function GenreGrid() {
  const t = useT()
  const picks = usePicks()
  const genres = picks?.genres ?? []
  const [open, setOpen] = useState<string | null>(null)
  const openGenre = genres.find((g) => g.name === open)

  return (
    <section id="genres" className="bg-ink px-4 pt-20 pb-24 text-paper lg:px-10 lg:pt-28 lg:pb-32" aria-labelledby="genres-title">
      <div className="mx-auto max-w-[1400px]">
        <div className="flex flex-wrap items-end justify-between gap-4 pb-10">
          <h2 id="genres-title" className="display text-display" data-reveal>
            {t('Pick a genre')}
          </h2>
          <p className="max-w-[40ch] text-body text-[#c4c4c4]" data-reveal style={{ '--i': 1 } as React.CSSProperties}>
            {picks?.live
              ? t("What's trending on YouTube today. Open one and your room starts playing it.")
              : t('Trailers people are watching right now. Open one and your room starts playing it.')}
          </p>
        </div>

        <ul className={cx('grid grid-cols-2 gap-2', genres.length % 4 === 0 ? 'sm:grid-cols-4' : 'sm:grid-cols-3 lg:grid-cols-5')}>
          {genres.map((g, i) => {
            const active = g.name === open
            return (
              <li key={g.name} data-reveal style={{ '--i': i % 5 } as React.CSSProperties}>
                <button
                  type="button"
                  aria-expanded={active}
                  aria-controls="genre-panel"
                  onClick={() => setOpen(active ? null : g.name)}
                  className={cx('genre-card group block w-full bg-night-2 text-left', active && 'is-active')}
                >
                  <span className="block aspect-video overflow-hidden bg-night-3">
                    <img src={still(g.videos[0]!, 'hq')} alt="" loading="lazy" className="genre-img size-full object-cover" />
                  </span>
                  <span className={cx('flex items-center justify-between gap-2 px-3 py-3 transition-colors sm:px-4', active ? 'bg-biro' : 'group-hover:bg-night-3')}>
                    <span className="text-body font-semibold tracking-[-0.01em] text-white">{t(g.name)}</span>
                    <ChevronDown
                      className={cx('size-4 shrink-0 text-white/80 transition-transform duration-300', active && 'rotate-180')}
                      aria-hidden="true"
                    />
                  </span>
                </button>
              </li>
            )
          })}
        </ul>

        <div id="genre-panel" role="region" aria-live="polite" aria-label={openGenre ? t(openGenre.name) : undefined}>
          {openGenre && (
            <div key={openGenre.name} className="pop-in mt-2 grid gap-2 bg-night-2 p-3 sm:p-5 lg:grid-cols-3">
              {openGenre.videos.map((v) => (
                <Pick key={v.id} video={v} />
              ))}
            </div>
          )}
        </div>
        <p className="pt-4 text-small text-[#a8a8a8]">{t('Thumbnails and videos are served by YouTube and belong to their owners.')}</p>
      </div>
    </section>
  )
}

function Pick({ video }: { video: GenreVideo }) {
  const t = useT()
  const create = useCreateRoom()
  const [name, setName] = useState(() => storage.getName())
  const [asking, setAsking] = useState(false)
  const url = `https://www.youtube.com/watch?v=${video.id}`

  const start = (e?: FormEvent) => {
    e?.preventDefault()
    const n = name.trim()
    if (!n) {
      setAsking(true)
      return
    }
    create.mutate({ name: n, roomName: t("{name}'s room", { name: n }), videoUrl: url })
  }

  return (
    <article className="flex flex-col bg-night-3">
      <div className="relative aspect-video overflow-hidden">
        <img
          src={video.thumb ?? thumb(video.id, 'mq')}
          srcSet={video.thumb ? undefined : `${thumb(video.id, 'mq')} 320w, ${thumb(video.id, 'hq')} 480w`}
          sizes="(min-width: 1024px) 30vw, 100vw"
          alt="" loading="lazy" className="size-full object-cover" />
        <Play className="absolute top-1/2 left-1/2 size-10 -translate-1/2 fill-white/90 text-white/90" aria-hidden="true" />
      </div>
      <div className="flex flex-1 flex-col gap-3 p-3">
        <div>
          <h3 className="line-clamp-2 text-base leading-snug font-medium text-fog">{video.title}</h3>
          <p className="pt-1 text-small text-haze">{video.channel}</p>
        </div>
        <form onSubmit={start} className="mt-auto flex flex-col gap-2" noValidate>
          {asking && (
            <input
              autoFocus
              value={name}
              maxLength={32}
              onChange={(e) => setName(e.target.value)}
              placeholder={t('Your name')}
              aria-label={t('Your name')}
              className="h-10 bg-night-4 px-3 text-fog placeholder:text-haze focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
            />
          )}
          <Button type="submit" tone="dark" size="sm" loading={create.isPending} icon={<ArrowRight className="size-4" />}>
            {t('Watch together')}
          </Button>
          {create.error && (
            <p role="alert" className="text-small text-alarm">
              {create.error instanceof ApiError ? create.error.message : t('Something went wrong. Try again.')}
            </p>
          )}
        </form>
      </div>
    </article>
  )
}
