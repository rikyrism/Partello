import { useEffect } from 'react'

/**
 * Staggered entrances for `[data-reveal]` elements. Content is visible without
 * JavaScript; the hidden start state only applies once `reveal-ready` is set.
 * Elements rendered later (e.g. once the trending list arrives) are picked up too.
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
    const added = new MutationObserver((records) => {
      for (const r of records)
        for (const node of r.addedNodes) {
          if (!(node instanceof Element)) continue
          if (node.matches('[data-reveal]:not(.is-in)')) io.observe(node)
          node.querySelectorAll('[data-reveal]:not(.is-in)').forEach((el) => io.observe(el))
        }
    })
    added.observe(document.body, { childList: true, subtree: true })
    return () => {
      added.disconnect()
      io.disconnect()
      root.classList.remove('reveal-ready')
    }
  }, [])
}
