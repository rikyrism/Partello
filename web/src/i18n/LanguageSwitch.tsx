import { cx } from '../lib/format'
import { useI18n, type Lang } from '.'

const LANGS: { id: Lang; label: string; name: string }[] = [
  { id: 'en', label: 'EN', name: 'English' },
  { id: 'id', label: 'ID', name: 'Bahasa Indonesia' },
]

/** Two-state language switch. `tone` matches the bar it sits in. */
export function LanguageSwitch({ tone = 'dark', className }: { tone?: 'dark' | 'light'; className?: string }) {
  const { lang, setLang } = useI18n()
  return (
    <div role="group" aria-label="Language / Bahasa" className={cx('flex shrink-0', className)}>
      {LANGS.map((l) => {
        const active = l.id === lang
        return (
          <button
            key={l.id}
            type="button"
            lang={l.id}
            aria-pressed={active}
            title={l.name}
            onClick={() => setLang(l.id)}
            className={cx(
              'h-9 w-10 text-small font-semibold transition-colors',
              tone === 'dark'
                ? active
                  ? 'bg-fog text-night'
                  : 'bg-night-3 text-haze hover:text-fog'
                : active
                  ? 'bg-ink text-paper'
                  : 'bg-surface text-muted hover:text-ink',
            )}
          >
            {l.label}
          </button>
        )
      })}
    </div>
  )
}
