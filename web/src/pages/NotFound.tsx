import { Link } from 'react-router'
import { Wordmark } from '../components/Wordmark'
import { useT } from '../i18n'

export function NotFound() {
  const t = useT()
  return (
    <main className="flex min-h-dvh flex-col bg-ink px-4 py-6 text-paper sm:px-8">
      <Link to="/" className="w-fit text-xl" aria-label="Partello home">
        <Wordmark />
      </Link>
      <div className="flex flex-1 flex-col justify-center gap-6 py-16">
        <h1 className="display text-head">{t('This page left the room')}</h1>
        <p className="max-w-md text-body text-[#c4c4c4]">{t("The address doesn't match anything on Partello.")}</p>
        <Link to="/" className="inline-flex h-12 w-fit items-center bg-biro px-5 font-medium text-white hover:bg-biro-deep">
          {t('Back to Partello')}
        </Link>
      </div>
    </main>
  )
}
