import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router'
import { ArrowRight } from 'lucide-react'
import { Button } from '../../components/Button'
import { Wordmark } from '../../components/Wordmark'
import { storage } from '../../lib/storage'
import { ApiError } from '../../lib/api'
import { RoomFragment } from './RoomFragment'
import { useScrollProgress } from './motion'
import { useT } from '../../i18n'
import { roomSlugFrom, useCreateRoom } from './useCreateRoom'

export function Stripe({ className = 'h-14' }: { className?: string }) {
  return (
    <div className={`flex ${className}`} aria-hidden="true">
      {Array.from({ length: 24 }, (_, i) => (
        <span key={i} className={i % 2 ? 'flex-1 bg-paper' : 'flex-1 bg-ink'} />
      ))}
    </div>
  )
}

export function Perforation({ className = 'text-ink/25' }: { className?: string }) {
  return (
    <div className={`flex justify-between overflow-hidden ${className}`} aria-hidden="true">
      {Array.from({ length: 40 }, (_, i) => (
        <span key={i} className="size-[5px] shrink-0 rounded-full bg-current" />
      ))}
    </div>
  )
}

export function Hero() {
  // Progress of the whole hero drives the stage's slight recede.
  const ref = useScrollProgress<HTMLElement>()
  return (
    <section ref={ref} id="top" className="flex flex-col bg-ink p-3 lg:grid lg:min-h-svh lg:grid-cols-2 lg:gap-3">
      <div className="order-2 lg:order-none">
        <Ticket />
      </div>
      <HeroStage />
    </section>
  )
}

function HeroStage() {
  return (
    // Phones: as tall as the room fragment itself, plus the strip the ticket's
    // stripe overlaps (-mt-6), so no call tile is cut off. Desktop: sticky stage.
    <div className="relative order-1 overflow-hidden pb-6 lg:sticky lg:top-3 lg:order-none lg:h-[calc(100svh-24px)] lg:pb-0">
      <RoomFragment className="hero-stage" />
    </div>
  )
}

/** The live thing: the ticket prints your name and really opens your room. */
function Ticket() {
  const t = useT()
  const [name, setName] = useState(() => storage.getName())
  const [mode, setMode] = useState<'create' | 'join'>('create')
  const [joinValue, setJoinValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const create = useCreateRoom()
  const navigate = useNavigate()
  const trimmed = name.trim()
  const roomName = trimmed ? t("{name}'s room", { name: trimmed }) : t('Your room')

  const submit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    if (mode === 'join') {
      const slug = roomSlugFrom(joinValue)
      if (!slug) return setError(t('Paste the invite link your friend sent, or its 12-character code.'))
      navigate(`/r/${slug}`)
      return
    }
    if (!trimmed) return setError(t('Write your name on the ticket first.'))
    create.mutate({ name: trimmed, roomName })
  }

  const apiError = create.error instanceof ApiError ? create.error.message : create.error ? t('Something went wrong. Try again.') : null

  return (
    <form
      onSubmit={submit}
      noValidate
      className="relative z-10 -mt-6 flex h-full flex-col bg-paper text-ink lg:mt-0 lg:min-h-[calc(100svh-24px)]"
    >
      <Stripe className="h-10 lg:h-14" />
      <div className="flex flex-1 flex-col items-center px-5 pt-10 pb-8 text-center sm:px-12 lg:justify-center lg:pt-12">
        <Wordmark className="text-heading [&>svg]:hidden" mark={false} />
        <h1 className="display mt-6 text-display" data-reveal>
          {t('Watch together.')}
          <br />
          {t('Wherever.')}
        </h1>
        <p className="mt-6 max-w-[38ch] text-body text-ink" data-reveal style={{ '--i': 1 } as React.CSSProperties}>
          {t('Paste a YouTube link, send friends the ticket, and press play for everyone at once. Talk and chat while you watch.')}
        </p>

        <div className="mt-10 w-full max-w-md text-left" data-reveal style={{ '--i': 2 } as React.CSSProperties}>
          <dl className="tabular grid grid-cols-[5.5rem_1fr] items-center gap-y-3 text-body">
            {mode === 'create' ? (
              <>
                <dt className="text-muted">
                  <label htmlFor="ticket-name">{t('Admit')}</label>
                </dt>
                <dd className="m-0">
                  <input
                    id="ticket-name"
                    value={name}
                    maxLength={32}
                    autoComplete="nickname"
                    onChange={(e) => {
                      setName(e.target.value)
                      setError(null)
                    }}
                    placeholder={t('Your name')}
                    aria-invalid={!!error}
                    aria-describedby={error ? 'ticket-error' : undefined}
                    className="h-12 w-full bg-surface px-3 text-body font-medium text-ink placeholder:font-normal placeholder:text-muted focus:outline-none focus-visible:outline-3 focus-visible:outline-biro"
                  />
                </dd>
                <dt className="text-muted">{t('Room')}</dt>
                <dd className="m-0 truncate font-medium" aria-live="polite">
                  {roomName}
                </dd>
              </>
            ) : (
              <>
                <dt className="text-muted">
                  <label htmlFor="ticket-join">{t('Invite')}</label>
                </dt>
                <dd className="m-0">
                  <input
                    id="ticket-join"
                    value={joinValue}
                    autoFocus
                    onChange={(e) => {
                      setJoinValue(e.target.value)
                      setError(null)
                    }}
                    placeholder={t('Link or room code')}
                    aria-invalid={!!error}
                    aria-describedby={error ? 'ticket-error' : undefined}
                    className="h-12 w-full bg-surface px-3 text-body text-ink placeholder:text-muted focus:outline-none focus-visible:outline-3 focus-visible:outline-biro"
                  />
                </dd>
              </>
            )}
            <dt className="text-muted">{t('Seat')}</dt>
            <dd className="m-0 font-medium">{t('Wherever you are')}</dd>
            <dt className="text-muted">{t('Price')}</dt>
            <dd className="m-0 font-medium">{t('Free, no sign-up')}</dd>
          </dl>
          {(error || apiError) && (
            <p id="ticket-error" role="alert" className="mt-3 text-base text-[#b3261e]">
              {error ?? apiError}
            </p>
          )}
        </div>

        <Perforation className="mt-8 w-full max-w-md text-ink/30" />

        <div className="mt-8 w-full max-w-md">
          <Button type="submit" size="lg" className="group w-full" loading={create.isPending}>
            {mode === 'create' ? t('Create a room') : t('Join the room')}
            <ArrowRight className="size-5 transition-transform duration-200 group-hover:translate-x-1" aria-hidden="true" />
          </Button>
        </div>

        <nav aria-label={t('More')} className="mt-7 flex flex-wrap justify-center gap-x-6 gap-y-2 text-base">
          <button
            type="button"
            className="underline-offset-4 hover:underline"
            onClick={() => {
              setMode((m) => (m === 'create' ? 'join' : 'create'))
              setError(null)
            }}
          >
            {mode === 'create' ? t('Join a room') : t('Create a room instead')}
          </button>
          <a href="#genres" className="underline-offset-4 hover:underline">
            {t('Pick a genre')}
          </a>
          <a href="/developers" className="underline-offset-4 hover:underline">
            {t('API for developers')}
          </a>
        </nav>
      </div>
    </form>
  )
}
