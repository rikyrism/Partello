// Motion primitives for the landing page. Everything moves with transform,
// opacity or clip-path, runs on requestAnimationFrame, and stands still under
// `prefers-reduced-motion` (content is always fully visible without JS).

import { useEffect, useRef, type CSSProperties, type ReactNode } from 'react'
import { cx } from '../../lib/format'

export const reducedMotion = () => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

/**
 * Writes `--p` on the element: 0 when its top meets the bottom of the
 * viewport, 1 when its bottom leaves the top. CSS turns it into parallax.
 */
export function useScrollProgress<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  useEffect(() => {
    const el = ref.current
    if (!el || reducedMotion()) return
    let frame = 0
    const update = () => {
      frame = 0
      const r = el.getBoundingClientRect()
      const vh = window.innerHeight
      const p = Math.min(1, Math.max(0, (vh - r.top) / (vh + r.height)))
      el.style.setProperty('--p', p.toFixed(4))
    }
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update)
    }
    update()
    window.addEventListener('scroll', onScroll, { passive: true })
    window.addEventListener('resize', onScroll)
    return () => {
      cancelAnimationFrame(frame)
      window.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', onScroll)
    }
  }, [])
  return ref
}

/**
 * Scroll velocity, smoothed and decaying, written as `--v` (-1..1) on the
 * element. `onVelocity` lets callers drive Web Animations as well.
 */
export function useScrollVelocity<T extends HTMLElement>(onVelocity?: (v: number) => void) {
  const ref = useRef<T>(null)
  const cb = useRef(onVelocity)
  useEffect(() => {
    cb.current = onVelocity
  }, [onVelocity])
  useEffect(() => {
    const el = ref.current
    if (!el || reducedMotion()) return
    let lastY = window.scrollY
    let lastT = performance.now()
    let v = 0
    let frame = requestAnimationFrame(function tick(now) {
      const y = window.scrollY
      const dt = Math.max(1, now - lastT)
      const instant = (y - lastY) / dt // px per ms
      lastY = y
      lastT = now
      v += (Math.max(-1, Math.min(1, instant / 3)) - v) * 0.12
      if (Math.abs(v) < 0.001) v = 0
      el.style.setProperty('--v', v.toFixed(3))
      cb.current?.(v)
      frame = requestAnimationFrame(tick)
    })
    return () => cancelAnimationFrame(frame)
  }, [])
  return ref
}

/**
 * A picture that opens like cinema curtains when it scrolls into view, then
 * keeps a gentle parallax. `data-curtain` is picked up by `useReveal`.
 */
export function CurtainImage({
  src,
  alt,
  className,
  imgClassName,
  parallax = true,
  style,
}: {
  src: string
  alt: string
  className?: string
  imgClassName?: string
  parallax?: boolean
  style?: CSSProperties
}) {
  const ref = useScrollProgress<HTMLDivElement>()

  // Each curtain watches itself, so remounted pictures open too.
  useEffect(() => {
    const el = ref.current
    if (!el) return
    if (!('IntersectionObserver' in window)) {
      el.classList.add('is-in')
      return
    }
    const io = new IntersectionObserver(
      ([e]) => {
        if (e?.isIntersecting) {
          el.classList.add('is-in')
          io.disconnect()
        }
      },
      { threshold: 0.15 },
    )
    io.observe(el)
    return () => io.disconnect()
  }, [ref])

  return (
    // The observed wrapper has no clip-path: Chrome counts a target's own
    // clip-path, so a closed curtain (zero area) would never "intersect".
    <div ref={ref} data-curtain className={cx('relative', className)} style={style}>
      <div className="curtain-clip absolute inset-0 overflow-hidden bg-ink">
        <img
          src={src}
          alt={alt}
          loading="lazy"
          className={cx('curtain-img size-full object-cover', parallax && 'parallax-img', imgClassName)}
        />
      </div>
    </div>
  )
}

/** Runs children only while visible (saves CPU for off-screen loops). */
export function useInView<T extends HTMLElement>(onChange: (visible: boolean) => void) {
  const ref = useRef<T>(null)
  const cb = useRef(onChange)
  useEffect(() => {
    cb.current = onChange
  }, [onChange])
  useEffect(() => {
    const el = ref.current
    if (!el || !('IntersectionObserver' in window)) return
    const io = new IntersectionObserver(([e]) => cb.current(!!e?.isIntersecting), { threshold: 0.15 })
    io.observe(el)
    return () => io.disconnect()
  }, [])
  return ref
}

export function Sprockets({ className }: { className?: string }) {
  return (
    <div className={cx('flex h-5 items-center gap-3 overflow-hidden px-2', className)} aria-hidden="true">
      {Array.from({ length: 80 }, (_, i) => (
        <span key={i} className="h-2.5 w-4 shrink-0 rounded-[2px] bg-paper/85" />
      ))}
    </div>
  )
}

export function FilmFrame({ children, caption }: { children: ReactNode; caption: string }) {
  return (
    <figure className="m-0 flex shrink-0 flex-col gap-2">
      <div className="film-frame relative aspect-video w-[clamp(15rem,28vw,26rem)] overflow-hidden bg-night-3">{children}</div>
      <figcaption className="w-[clamp(15rem,28vw,26rem)] truncate text-small text-[#c4c4c4]">{caption}</figcaption>
    </figure>
  )
}
