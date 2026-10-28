import { useCallback, useEffect, useState, useSyncExternalStore } from 'react'

/** Whether a CSS media query matches, updated live. False where matchMedia is missing. */
export function useMedia(query: string): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      const list = window.matchMedia?.(query)
      list?.addEventListener('change', onChange)
      return () => list?.removeEventListener('change', onChange)
    },
    [query],
  )
  return useSyncExternalStore(subscribe, () => window.matchMedia?.(query).matches ?? false, () => false)
}

const TEXT_INPUTS = new Set(['text', 'search', 'url', 'email', 'password', 'tel', 'number'])

const typing = () => {
  const el = document.activeElement
  return el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && TEXT_INPUTS.has(el.type)) || (el instanceof HTMLElement && el.isContentEditable)
}

/**
 * The height left above the on-screen keyboard while someone types on a
 * touch device, or null when no keyboard is up. iOS never shrinks the layout
 * viewport for the keyboard (it scrolls the page instead), so the room sizes
 * itself to this and keeps the page pinned to the top.
 */
export function useSoftKeyboard(): number | null {
  const touch = useMedia('(pointer: coarse)')
  const [height, setHeight] = useState<number | null>(null)

  useEffect(() => {
    const vv = window.visualViewport
    if (!touch || !vv) return
    const update = () => {
      if (!typing()) return setHeight(null)
      setHeight(Math.round(vv.height))
      if (window.scrollY !== 0) window.scrollTo(0, 0)
    }
    vv.addEventListener('resize', update)
    vv.addEventListener('scroll', update)
    document.addEventListener('focusin', update)
    document.addEventListener('focusout', update)
    return () => {
      vv.removeEventListener('resize', update)
      vv.removeEventListener('scroll', update)
      document.removeEventListener('focusin', update)
      document.removeEventListener('focusout', update)
    }
  }, [touch])

  return touch ? height : null
}
