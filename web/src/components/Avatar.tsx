import { cx, hueFor, initials } from '../lib/format'

export function Avatar({ id, name, size = 'md', className }: { id: string; name: string; size?: 'sm' | 'md' | 'lg'; className?: string }) {
  const hue = hueFor(id)
  return (
    <span
      aria-hidden="true"
      className={cx(
        'inline-flex shrink-0 select-none items-center justify-center font-semibold',
        size === 'sm' && 'size-7 text-xs',
        size === 'md' && 'size-9 text-sm',
        size === 'lg' && 'size-20 text-2xl',
        className,
      )}
      style={{ background: `hsl(${hue} 45% 32%)`, color: `hsl(${hue} 80% 92%)` }}
    >
      {initials(name)}
    </span>
  )
}
