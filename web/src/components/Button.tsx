import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { cx } from '../lib/format'

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'ink'
type Size = 'sm' | 'md' | 'lg'

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
  size?: Size
  loading?: boolean
  icon?: ReactNode
  /** Room pages are dark; public pages are light. */
  tone?: 'light' | 'dark'
}

const base =
  'inline-flex items-center justify-center gap-2 font-medium select-none whitespace-nowrap ' +
  'transition-[background-color,color,transform] duration-150 active:translate-y-px ' +
  'disabled:opacity-45 disabled:active:translate-y-0'

const sizes: Record<Size, string> = {
  sm: 'h-9 px-3 text-[0.9375rem]',
  md: 'h-11 px-4 text-base',
  lg: 'h-14 px-6 text-lg',
}

const variants: Record<'light' | 'dark', Record<Variant, string>> = {
  light: {
    primary: 'bg-biro text-white hover:bg-biro-deep',
    ink: 'bg-ink text-paper hover:bg-[#2a2a2a]',
    secondary: 'bg-surface text-ink hover:bg-[#e8e8e8]',
    ghost: 'bg-transparent text-ink hover:bg-surface',
    danger: 'bg-[#c62828] text-white hover:bg-[#a31f1f]',
  },
  dark: {
    primary: 'bg-biro text-white hover:bg-[#2c5cf0]',
    ink: 'bg-fog text-night hover:bg-white',
    secondary: 'bg-night-3 text-fog hover:bg-night-4',
    ghost: 'bg-transparent text-fog hover:bg-night-3',
    danger: 'bg-[#b3261e] text-white hover:bg-[#c9352c]',
  },
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'primary', size = 'md', loading, icon, tone = 'light', className, children, disabled, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      className={cx(base, sizes[size], variants[tone][variant], className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...rest}
    >
      {loading ? <Spinner /> : icon}
      {children}
    </button>
  )
})

export function Spinner({ className }: { className?: string }) {
  return (
    <svg className={cx('size-4 animate-spin', className)} viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeOpacity="0.3" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" fill="none" stroke="currentColor" strokeWidth="3" />
    </svg>
  )
}

/** Square icon-only button; `label` is required for screen readers. */
export const IconButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean; tone?: 'light' | 'dark'; size?: 'sm' | 'md' }
>(function IconButton({ label, active, tone = 'dark', size = 'md', className, children, ...rest }, ref) {
  return (
    <button
      ref={ref}
      aria-label={label}
      title={label}
      className={cx(
        'inline-flex shrink-0 items-center justify-center transition-[background-color,color,transform] duration-150 active:translate-y-px disabled:opacity-40',
        size === 'md' ? 'size-11' : 'size-9',
        tone === 'dark'
          ? active
            ? 'bg-fog text-night hover:bg-white'
            : 'bg-night-3 text-fog hover:bg-night-4'
          : active
            ? 'bg-ink text-paper'
            : 'bg-surface text-ink hover:bg-[#e8e8e8]',
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  )
})
