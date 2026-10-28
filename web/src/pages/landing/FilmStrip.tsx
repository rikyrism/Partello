// A strip of film running past the screen. It drifts on its own, speeds up
// and leans with your scroll, and pauses under the pointer. Frames are
// public-domain films (credited in the footer), shown as examples of what a
// room can play.

import { useCallback, useRef } from 'react'
import { FilmFrame, Sprockets, reducedMotion, useInView, useScrollVelocity } from './motion'
import { GENRES, thumb } from './genres'
import { useT } from '../../i18n'

// One trending pick per genre, in colour.
const FRAMES = GENRES.map((g) => ({ src: thumb(g.videos[0]!.id), genre: g.name, title: g.videos[0]!.title }))

export function FilmStrip() {
  const t = useT()
  const trackRef = useRef<HTMLDivElement>(null)
  const visible = useRef(false)

  const onVelocity = useCallback((v: number) => {
    const anim = trackRef.current?.getAnimations()[0]
    if (!anim || !visible.current) return
    // Scrolling down pushes the film faster; scrolling up runs it backwards briefly.
    anim.playbackRate = 1 + v * 6
  }, [])

  const velocityRef = useScrollVelocity<HTMLDivElement>(onVelocity)
  const viewRef = useInView<HTMLElement>((v) => {
    visible.current = v
    const anim = trackRef.current?.getAnimations()[0]
    if (!anim || reducedMotion()) return
    if (v) anim.play()
    else anim.pause()
  })

  // Two identical runs make the loop seamless (the animation moves by -50%).
  const run = [...FRAMES, ...FRAMES]

  return (
    <section ref={viewRef} className="overflow-hidden bg-ink py-16 text-paper lg:py-24" aria-label={t('Anything on YouTube, in sync')}>
      <p className="max-w-[46ch] px-4 pb-8 text-lead font-medium lg:px-10" data-reveal>
        {t("Anything on YouTube plays here. Trailers, music videos, a friend's upload, the match highlights.")}
      </p>
      <div ref={velocityRef} className="film-lean">
        <div className="bg-[#1d1d1d] py-2">
          <Sprockets />
          <div className="group overflow-hidden py-3">
            <div ref={trackRef} className="film-track flex w-max gap-4 px-2 group-hover:[animation-play-state:paused]">
              {[...run, ...run].map((f, i) => (
                <FilmFrame key={i} caption={`${t(f.genre)} · ${f.title.split('|')[0]!.trim()}`}>
                  <img src={f.src} alt={i < FRAMES.length ? f.title : ''} loading="lazy" className="film-img size-full object-cover" />
                </FilmFrame>
              ))}
            </div>
          </div>
          <Sprockets />
        </div>
      </div>
    </section>
  )
}
