import { useEffect } from 'react'

/**
 * Staggered entrances for `[data-reveal]` elements. Content is visible without
 * JavaScript; the hidden start state only applies once `reveal-ready` is set.
 */
export function useReveal() {
  useEffect(() => {
    if (!('IntersectionObserver' in window)) return
    const root = document.documentElement
    root.classList.add('reveal-ready')
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            e.target.classList.add('is-in')
            io.unobserve(e.target)
          }
        }
      },
      { rootMargin: '0px 0px -8% 0px', threshold: 0.1 },
    )
    document.querySelectorAll('[data-reveal]').forEach((el) => io.observe(el))
    return () => {
      io.disconnect()
      root.classList.remove('reveal-ready')
    }
  }, [])
}
