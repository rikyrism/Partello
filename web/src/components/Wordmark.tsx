import { cx } from '../lib/format'

/** The Partello wordmark: a play-shaped "P" counter and the name. */
export function Wordmark({ className, mark = true }: { className?: string; mark?: boolean }) {
  return (
    <span className={cx('inline-flex items-center gap-2 font-semibold tracking-[-0.03em]', className)}>
      {mark && (
        <svg viewBox="0 0 24 24" className="size-[1.05em]" aria-hidden="true">
          <rect x="0" y="0" width="5" height="24" fill="currentColor" />
          <path d="M8 3v18l14-9z" fill="currentColor" />
        </svg>
      )}
      <span>Partello</span>
    </span>
  )
}
