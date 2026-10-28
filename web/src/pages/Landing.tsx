import { Link } from 'react-router'
import { Wordmark } from '../components/Wordmark'
import { Hero } from './landing/Hero'
import { FilmStrip } from './landing/FilmStrip'
import { ClosingCall, Footer, Itinerary, Questions, Sheets, Showcase } from './landing/Sections'
import { useReveal } from './landing/useReveal'
import { GenreGrid } from './landing/GenreGrid'
import { useT } from '../i18n'
import { LanguageSwitch } from '../i18n/LanguageSwitch'

export function Landing() {
  const t = useT()
  useReveal()
  return (
    <>
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:bg-paper focus:px-3 focus:py-2">
        {t('Skip to content')}
      </a>
      <header className="flex h-14 items-center gap-6 bg-ink px-4 text-paper lg:px-6">
        <Link to="/" className="text-body" aria-label="Partello home">
          <Wordmark />
        </Link>
        <nav aria-label={t('Main')} className="ml-auto flex items-center gap-3 text-base sm:gap-5">
          <a href="#genres" className="hidden hover:underline md:inline">
            {t('Pick a genre')}
          </a>
          <a href="#how" className="hidden hover:underline md:inline">
            {t('How it works')}
          </a>
          <Link to="/developers" className="hidden hover:underline md:inline">
            {t('Developers')}
          </Link>
          <LanguageSwitch />
          <a href="#top" className="inline-flex h-9 items-center bg-biro px-3 font-medium text-white hover:bg-biro-deep">
            {t('Create a room')}
          </a>
        </nav>
      </header>
      <main id="main">
        <Hero />
        <GenreGrid />
        <FilmStrip />
        <Showcase />
        <Itinerary />
        <Sheets />
        <Questions />
        <ClosingCall />
      </main>
      <Footer />
    </>
  )
}
