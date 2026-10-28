import { useState, type FormEvent } from 'react'
import { Link } from 'react-router'
import { ApiError, type RoomSummary } from '../lib/api'
import { storage } from '../lib/storage'
import { Button } from '../components/Button'
import { Wordmark } from '../components/Wordmark'
import { useT } from '../i18n'
import { LanguageSwitch } from '../i18n/LanguageSwitch'
import { Lock } from 'lucide-react'

/** Shown to anyone opening an invite link without a session: pick a name, join. */
export function JoinGate({
  slug,
  summary,
  loading,
  onJoin,
}: {
  slug: string
  summary?: RoomSummary
  loading: boolean
  onJoin: (name: string, pin?: string) => Promise<void>
}) {
  const t = useT()
  const [name, setName] = useState(() => storage.getName())
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [pin, setPin] = useState('')
  const [askPin, setAskPin] = useState(false)
  const isHost = !!storage.getHostKey(slug)
  // The host key opens a PIN room without the PIN.
  const needsPin = !isHost && (askPin || !!summary?.room.has_password)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) {
      setError(t('Enter a name so people know who you are.'))
      return
    }
    if ([...trimmed].length > 32) {
      setError(t('Keep it to 32 characters or fewer.'))
      return
    }
    if (needsPin && !/^\d{4}$/.test(pin)) {
      setError(t('Enter the 4-digit room PIN.'))
      return
    }
    setBusy(true)
    setError(null)
    try {
      await onJoin(trimmed, needsPin ? pin : undefined)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'pin_required') setAskPin(true)
      setError(err instanceof ApiError ? err.message : t('Something went wrong. Try again.'))
      setBusy(false)
    }
  }

  const count = summary?.participant_count ?? 0
  return (
    <main className="room-scope flex min-h-dvh flex-col bg-night px-4 py-6 text-fog sm:px-8">
      <div className="flex items-center justify-between">
        <Link to="/" className="w-fit text-xl" aria-label="Partello home">
          <Wordmark />
        </Link>
        <LanguageSwitch />
      </div>
      <div className="flex flex-1 items-center py-12">
        <form onSubmit={submit} className="w-full max-w-xl bg-fog text-night" noValidate>
          <div className="flex h-10 sm:h-14" aria-hidden="true">
            {Array.from({ length: 16 }, (_, i) => (
              <span key={i} className={i % 2 ? 'flex-1 bg-fog' : 'flex-1 bg-night'} />
            ))}
          </div>
          <div className="flex flex-col gap-5 p-6 sm:p-10">
            <div>
              <p className="text-body text-[#4a4a4a]">{isHost ? t('Welcome back to your room') : t("You're invited to")}</p>
              <h1 className="display pt-2 text-[clamp(2rem,6vw,3.25rem)] break-words">
                {loading ? <span className="inline-block h-[0.9em] w-2/3 animate-pulse bg-[#dcdcdc] align-middle" /> : summary?.room.name ?? t('A Partello room')}
              </h1>
              <p className="pt-3 text-[#4a4a4a]">
                {loading ? ' ' : count === 0 ? t('Nobody is here yet. You are first.') : count === 1 ? t('1 person is here right now.') : t('{n} people are here right now.', { n: count })}
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <label htmlFor="display-name" className="font-medium">
                {t('Your name')}
              </label>
              <input
                id="display-name"
                autoFocus
                autoComplete="nickname"
                maxLength={32}
                value={name}
                onChange={(e) => {
                  setName(e.target.value)
                  setError(null)
                }}
                placeholder={t('How friends will see you')}
                aria-invalid={!!error}
                aria-describedby={error ? 'join-error' : undefined}
                className="h-14 bg-[#e6e6e6] px-4 text-lg text-night placeholder:text-[#6b6b6b] focus:outline-none focus-visible:outline-3 focus-visible:outline-biro"
              />
              {needsPin && (
                <>
                  <label htmlFor="room-pin" className="mt-3 flex items-center gap-2 font-medium">
                    <Lock className="size-4" aria-hidden="true" /> {t('Room PIN')}
                  </label>
                  <input
                    id="room-pin"
                    type="password"
                    inputMode="numeric"
                    autoComplete="off"
                    pattern="[0-9]{4}"
                    maxLength={4}
                    value={pin}
                    onChange={(e) => {
                      setPin(e.target.value.replace(/\D/g, '').slice(0, 4))
                      setError(null)
                    }}
                    placeholder="••••"
                    aria-describedby="pin-hint"
                    aria-invalid={!!error}
                    className="tabular h-14 w-40 bg-[#e6e6e6] px-4 text-center text-2xl tracking-[0.5em] text-night placeholder:text-[#6b6b6b] focus:outline-none focus-visible:outline-3 focus-visible:outline-biro"
                  />
                  <p id="pin-hint" className="text-small text-[#4a4a4a]">
                    {t('Ask the host for the 4-digit PIN.')}
                  </p>
                </>
              )}
              {error && (
                <p id="join-error" role="alert" className="text-[0.9375rem] text-[#b3261e]">
                  {error}
                </p>
              )}
            </div>
            <Button type="submit" size="lg" loading={busy} disabled={summary?.is_full}>
              {summary?.is_full ? t('Room is full') : t('Join the room')}
            </Button>
            <p className="text-small text-[#4a4a4a]">{t('No account needed. Your camera and microphone stay off until you join a call.')}</p>
          </div>
        </form>
      </div>
    </main>
  )
}
