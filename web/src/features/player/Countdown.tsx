import { useEffect, useState } from 'react'
import { useController, useRoom } from '../../room/context'
import { useT } from '../../i18n'

/** The big 3-2-1 over the video before a synchronized start. */
export function CountdownOverlay() {
  const t = useT()
  const ctrl = useController()
  const countdown = useRoom((s) => s.countdown)
  const [now, setNow] = useState(() => ctrl.clock.now())

  useEffect(() => {
    if (!countdown) return
    const tick = () => setNow(ctrl.clock.now())
    const id = setInterval(tick, 100)
    return () => clearInterval(id)
  }, [countdown, ctrl])

  if (!countdown) return null
  const left = countdown.startAt - now
  // Show "Go!" briefly after zero, until the play event arrives.
  if (left < -1200) return null
  const n = Math.ceil(left / 1000)
  const label = n > 0 ? String(Math.min(n, 3)) : t('Go!')

  return (
    <div className="pointer-events-none absolute inset-0 grid place-items-center bg-black/55" role="status" aria-live="assertive">
      <span key={label} className="countdown-pop display text-[clamp(5rem,22vw,14rem)] leading-none text-white">
        {label}
      </span>
    </div>
  )
}
