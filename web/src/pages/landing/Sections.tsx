import { useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode } from 'react'
import { Link } from 'react-router'
import { Mic, Pause, Play, Plus, Send } from 'lucide-react'
import { ApiError } from '../../lib/api'
import { cx } from '../../lib/format'
import { Wordmark } from '../../components/Wordmark'
import { Perforation, Stripe } from './Hero'
import { useCreateRoom } from './useCreateRoom'
import { CurtainImage, reducedMotion, useInView } from './motion'
import { FEATURED, thumb } from './genres'
import { msg, useT } from '../../i18n'

const delay = (i: number) => ({ '--i': i }) as CSSProperties

// ---------------------------------------------------------------- showcase

const SYNC_TABS = [
  { id: 'pause', label: msg('Pause'), body: msg('When the host pauses, every screen in the room stops on the same frame. Nobody hears the punchline first.') },
  { id: 'seek', label: msg('Seek'), body: msg('Skip back to replay a scene and everyone jumps with you, down to the second.') },
  { id: 'late', label: msg('Late arrivals'), body: msg('Friends who open the link halfway through land on the exact moment everyone else is watching.') },
] as const

export function Showcase() {
  const t = useT()
  const [tab, setTab] = useState<(typeof SYNC_TABS)[number]['id']>('pause')
  const active = SYNC_TABS.find((t) => t.id === tab)!
  return (
    <section className="bg-surface lg:grid lg:min-h-[88svh] lg:grid-cols-2" aria-labelledby="showcase-title">
      <div className="flex items-center justify-center p-4 lg:p-10">
        <figure className="relative m-0 aspect-square w-full max-w-[42rem]">
          <CurtainImage
            key={tab}
            src={thumb(FEATURED.spongebob.id, 'maxres')}
            alt={FEATURED.spongebob.title}
            className="size-full"
            imgClassName={tab === 'seek' ? 'showcase-seek' : tab === 'late' ? 'showcase-late' : ''}
          />
          <figcaption className="absolute bottom-0 left-0 flex items-center gap-2 bg-ink px-3 py-2 text-base text-paper">
            {tab === 'pause' ? <Pause className="size-4" aria-hidden="true" /> : <Play className="size-4" aria-hidden="true" />}
            <span className="tabular">
              SpongeBob · {tab === 'seek' ? t('jumped to 0:42 on every screen') : tab === 'late' ? t('Molly joins at 1:24, on every screen') : t('1:24 on every screen')}
            </span>
          </figcaption>
        </figure>
      </div>
      <div className="flex flex-col justify-between gap-12 px-4 pt-6 pb-4 lg:py-10 lg:pr-10 lg:pl-4">
        <div role="tablist" aria-label={t('What stays in sync')} className="flex flex-wrap gap-x-6 gap-y-2">
          {SYNC_TABS.map((tabItem) => (
            <button
              key={tabItem.id}
              role="tab"
              aria-selected={tab === tabItem.id}
              onClick={() => setTab(tabItem.id)}
              className={cx(
                'text-body transition-colors',
                tab === tabItem.id ? 'font-semibold text-ink underline decoration-biro decoration-2 underline-offset-8' : 'text-muted hover:text-ink',
              )}
            >
              {t(tabItem.label)}
            </button>
          ))}
        </div>
        <div>
          <h2 id="showcase-title" className="display text-display" data-reveal>
            {t('One remote, many sofas')}
          </h2>
          <p className="mt-8 max-w-[34ch] text-body" role="tabpanel" aria-live="polite" data-reveal style={delay(1)}>
            {t(active.body)}
          </p>
        </div>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2" data-reveal style={delay(2)}>
          <a href="#top" className="flex h-[60px] items-center justify-center bg-ink font-medium text-paper transition-colors hover:bg-[#2a2a2a] lg:h-[72px]">
            {t('Print my ticket')}
          </a>
          <a href="#questions" className="flex h-[60px] items-center justify-center bg-[#e4e4e4] font-medium text-ink transition-colors hover:bg-[#d8d8d8] lg:h-[72px]">
            {t('How exact is it?')}
          </a>
        </div>
      </div>
    </section>
  )
}

// ---------------------------------------------------------------- how it works

const ITINERARY: { stage: string; rows: [string, string][] }[] = [
  {
    stage: msg('Before'),
    rows: [
      [msg('Write your name on the ticket'), msg('5 sec')],
      [msg('Press "Create a room"'), msg('1 click')],
    ],
  },
  {
    stage: msg('Invite'),
    rows: [
      [msg('Copy the invite link'), msg('1 click')],
      [msg('Friends open it and type a name'), msg('10 sec')],
    ],
  },
  {
    stage: msg('Showtime'),
    rows: [
      [msg('Paste a YouTube link in the queue'), msg('5 sec')],
      [msg('Join the call, talk while you watch'), msg('all night')],
    ],
  },
]

export function Itinerary() {
  const t = useT()
  return (
    <section id="how" className="bg-ink px-4 py-24 text-paper lg:px-10 lg:py-40" aria-labelledby="how-title">
      <div className="mx-auto grid max-w-[1400px] gap-12 lg:grid-cols-12">
        <div className="lg:col-span-5">
          <h2 id="how-title" className="display text-display" data-reveal>
            {t('Under a minute to showtime')}
          </h2>
          <p className="mt-8 max-w-[30ch] text-body text-paper" data-reveal style={delay(1)}>
            {t('No downloads, no accounts, no browser extension. A room is a link; whoever has it can sit down.')}
          </p>
        </div>
        <div className="lg:col-span-5 lg:col-start-7" data-reveal style={delay(2)}>
          <div className="zigzag-bottom w-full max-w-[480px] bg-paper px-6 pt-6 pb-10 text-ink">
            <div className="flex items-baseline justify-between">
              <Wordmark className="text-body" />
              <span className="tabular text-base text-muted">{t('Admits everyone you invite')}</span>
            </div>
            {ITINERARY.map((group) => (
              <div key={group.stage} className="pt-7">
                <p className="font-semibold">{t(group.stage)}</p>
                <ul className="mt-2 flex flex-col gap-2">
                  {group.rows.map(([what, when]) => (
                    <li key={what} className="tabular flex items-end gap-2 text-base">
                      <span>{t(what)}</span>
                      <span className="mb-[0.4em] flex-1 border-b-4 border-dotted border-ink/30" aria-hidden="true" />
                      <span className="shrink-0 font-medium">{t(when)}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
            <p className="tabular mt-8 flex h-11 items-center justify-between bg-ink px-3 text-paper">
              <span>{t('Everyone on the same frame')}</span>
              <span className="font-medium">{t('≈ 30 sec')}</span>
            </p>
          </div>
        </div>
      </div>
    </section>
  )
}

// ---------------------------------------------------------------- feature sheets

interface Sheet {
  title: string
  body: string
  bg: string
  ink: string
  visual: ReactNode
}

const SHEETS: Sheet[] = [
  {
    title: msg('The same frame on every screen'),
    body: msg('The room keeps one clock. Your player checks it every half second and quietly catches up if your connection stutters, so nobody drifts ahead.'),
    bg: 'bg-paper',
    ink: 'text-ink',
    visual: <SyncVisual />,
  },
  {
    title: msg('A chat that belongs to the room'),
    body: msg('Every room has its own chat, with a short history for people who join late. Messages arrive as they are sent, without a refresh.'),
    bg: 'bg-biro',
    ink: 'text-white',
    visual: <ChatVisual />,
  },
  {
    title: msg('Talk, or turn the camera on'),
    body: msg('Join the call with one click. Microphone and camera only switch on when you ask, and audio and video go straight between browsers.'),
    bg: 'bg-[#e9e9e9]',
    ink: 'text-ink',
    visual: <CallVisual />,
  },
  {
    title: msg('An API to open rooms from your site'),
    body: msg('Get a key, create rooms and queue videos from your own server or bot. Your users land in a Partello room that is already playing.'),
    bg: 'bg-ink',
    ink: 'text-paper',
    visual: <ApiVisual />,
  },
]

export function Sheets() {
  const t = useT()
  return (
    <section className="bg-paper pt-24 lg:pt-40" aria-labelledby="sheets-title">
      <h2 id="sheets-title" className="display px-4 pb-12 text-display lg:px-10" data-reveal>
        {t('Movie night, sorted')}
      </h2>
      <div>
        {SHEETS.map((s, i) => (
          <article
            key={s.title}
            // Folders stack only on wide screens; on phones and tablets they read in plain order.
            // Earlier folders are taller by the offsets above later ones, so every
            // folder's bottom edge lines up and the stack leaves the screen as one.
            className={cx('flex flex-col lg:sticky lg:grid lg:min-h-(--sheet-h) lg:grid-cols-12 lg:grid-rows-[auto_1fr]', s.bg, s.ink)}
            style={{
              top: `calc(${i} * var(--sheet-band))`,
              ['--sheet-band' as string]: 'clamp(52px, 6vw, 72px)',
              ['--sheet-h' as string]: `calc(72svh + ${SHEETS.length - 1 - i} * var(--sheet-band))`,
            }}
          >
            <h3 className="flex h-[clamp(52px,6vw,72px)] items-center px-4 text-heading font-semibold tracking-[-0.03em] lg:col-span-12 lg:px-10">
              {t(s.title)}
            </h3>
            <div className="order-2 px-5 pt-6 pb-10 lg:order-none lg:col-span-5 lg:px-10 lg:pt-8">
              <p className="max-w-[40ch] text-body leading-relaxed">{t(s.body)}</p>
            </div>
            <div className="order-1 min-h-[16rem] lg:order-none lg:col-span-7 lg:h-[calc(72svh-var(--sheet-band))]">{s.visual}</div>
          </article>
        ))}
      </div>
    </section>
  )
}

function SyncVisual() {
  const t = useT()
  const screens = [t("Nita's laptop"), t("Riky's phone"), t("Molly's TV")]
  const [clock, setClock] = useState(61.2)
  const running = useRef(false)
  const ref = useInView<HTMLDivElement>((v) => (running.current = v))

  // One clock drives all three screens: that is the whole point.
  useEffect(() => {
    if (reducedMotion()) return
    const id = setInterval(() => running.current && setClock((x) => (x >= 140 ? 61.2 : x + 0.1)), 100)
    return () => clearInterval(id)
  }, [])

  const stamp = `${Math.floor(clock / 60)}:${(clock % 60).toFixed(1).padStart(4, '0')}`
  return (
    <div ref={ref} data-ui className="grid size-full grid-cols-1 gap-2 bg-ink p-2 sm:grid-cols-3">
      {screens.map((who, i) => (
        <div key={who} className="flex flex-col bg-night-2">
          <div className="relative aspect-video overflow-hidden sm:aspect-auto sm:flex-1">
            <img
              src={thumb(FEATURED.wildwood.id, 'maxres')}
              alt={i === 0 ? t('The same moment of the Wildwood trailer on three screens') : ''}
              className="drift absolute inset-0 size-full object-cover"
              loading="lazy"
            />
          </div>
          <div className="h-1 bg-night-4">
            <div className="sync-bar h-full bg-biro-bright" />
          </div>
          <div className="tabular flex items-center justify-between px-3 py-2 text-base text-fog">
            <span>{who}</span>
            <span className="text-biro-bright">{stamp}</span>
          </div>
        </div>
      ))}
    </div>
  )
}

function ChatVisual() {
  const t = useT()
  const msgs = [
    ['Nita', msg('who brought the krabby patties')],
    ['Riky', msg('patrick is me before coffee')],
    ['Molly', msg('skip to the flying dutchman part?')],
    ['Nita', msg('yes!!')],
  ]
  return (
    <div data-ui className="flex size-full flex-col justify-end gap-3 bg-[#1238b0] p-5 text-white lg:p-8">
      <CurtainImage
        src={thumb(FEATURED.spongebobTeaser.id, 'maxres')}
        alt={FEATURED.spongebobTeaser.title}
        className="mb-2 min-h-40 w-full flex-1"
      />
      {msgs.map(([who, body], i) => (
        <p key={i} className="max-w-md bg-white/10 px-4 py-2.5 text-base" data-reveal style={delay(i + 1)}>
          <span className="font-semibold">{who}</span> {t(body!)}
        </p>
      ))}
      <div className="flex max-w-md items-center justify-between bg-white px-4 py-3 text-base text-muted">
        Message the room <Send className="size-4 text-biro" aria-hidden="true" />
      </div>
    </div>
  )
}

function CallVisual() {
  const t = useT()
  return (
    <div data-ui className="grid size-full grid-cols-2 grid-rows-[1fr_auto] gap-2 bg-ink p-2">
      <div className="relative col-span-2 min-h-48">
        <CurtainImage src={thumb(FEATURED.forgottenIsland.id, 'maxres')} alt={FEATURED.forgottenIsland.title} className="size-full" />
        <span className="speaking-ring pointer-events-none absolute inset-0" aria-hidden="true" />
      </div>
      {[
        ['Nita', 200, msg('Speaking')],
        ['Riky', 30, msg('Muted')],
      ].map(([n, hue, state]) => (
        <div key={n as string} className="flex items-center gap-3 bg-night-2 p-3 text-fog">
          <span className="grid size-10 place-items-center font-semibold" style={{ background: `hsl(${hue} 45% 32%)`, color: `hsl(${hue} 80% 92%)` }}>
            {(n as string)[0]}
          </span>
          <span className="flex-1">{n}</span>
          <span className="inline-flex items-center gap-1 text-base text-haze">
            <Mic className="size-4" aria-hidden="true" /> {t(state as string)}
          </span>
        </div>
      ))}
    </div>
  )
}

function ApiVisual() {
  return (
    <div data-ui className="flex size-full items-center bg-night-2 p-5 lg:p-10">
      <pre className="w-full overflow-x-auto bg-night p-5 text-base leading-relaxed text-fog">
        <code>{`curl -X POST https://partello.example/api/v1/rooms \\
  -H "Authorization: Bearer $PARTELLO_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{"name": "Launch party",
       "video_url": "https://youtu.be/dQw4w9WgXcQ"}'

{
  "invite_url": "https://partello.example/r/k7Qm2xPa9Rt4",
  "host_url":   "https://partello.example/r/k7Qm2xPa9Rt4#host=…"
}`}</code>
      </pre>
    </div>
  )
}

// ---------------------------------------------------------------- questions

const FAQ: [string, string][] = [
  [msg('Is Partello free?'), msg('Yes. Creating rooms, chatting and calling cost nothing, and there is no account to make.')],
  [msg('Do my friends need to sign up?'), msg('No. They open the invite link, type a name, and they are in.')],
  [msg('Which videos can we watch?'), msg('Any YouTube video whose owner allows playing it on other sites. Paste the normal YouTube link, a youtu.be short link, or a Shorts link.')],
  [msg('Why does a video refuse to play?'), msg("Some owners block embedding, and private or removed videos can't play anywhere. Partello checks this when you add a link and tells you straight away.")],
  [msg('How many people fit in a room?'), msg('Up to 16 people can watch together, and up to 8 can be in the call at once.')],
  [msg('Who controls playback?'), msg('The host, by default. The host can let everyone play, pause and skip, or hand the host role to someone else.')],
  [msg('How exact is the sync?'), msg('Usually within a second. Each player compares itself with the room clock twice a second and corrects drift; a very slow connection may lag briefly while it buffers.')],
  [msg('Is the call private?'), msg('Voice and camera travel directly between browsers, encrypted by WebRTC. Partello only passes along the messages needed to connect you.')],
  [msg('What if my connection drops?'), msg('Partello reconnects by itself and puts you back at the right moment. You keep your seat for a few seconds while it does.')],
  [msg('Do you keep our chat?'), msg('A room keeps its recent messages so late arrivals can catch up. Rooms nobody visits for 30 days are deleted with their chat.')],
  [msg('Does it work on phones?'), msg('Yes, in a current mobile browser. Some phones ask you to tap once before video plays with sound.')],
  [msg('Does Partello download videos?'), msg("Never. Videos play in YouTube's own player, straight from YouTube.")],
]

export function Questions() {
  const t = useT()
  const [open, setOpen] = useState<number | null>(0)
  return (
    <section id="questions" className="bg-paper px-4 py-24 lg:px-10 lg:py-40" aria-labelledby="faq-title">
      <div className="mx-auto grid max-w-[1400px] gap-10 lg:grid-cols-12">
        <h2 id="faq-title" className="display text-heading lg:col-span-3" data-reveal>
          {t('Good questions')}
        </h2>
        <ul className="flex flex-col gap-2 lg:col-span-8 lg:col-start-5" data-reveal style={delay(1)}>
          {FAQ.map(([q, a], i) => {
            const expanded = open === i
            return (
              <li key={q} className={cx('transition-colors duration-300', expanded ? 'bg-surface' : 'bg-paper hover:bg-surface')}>
                <h3 className="m-0">
                  <button
                    type="button"
                    id={`faq-q-${i}`}
                    aria-expanded={expanded}
                    aria-controls={`faq-a-${i}`}
                    onClick={() => setOpen(expanded ? null : i)}
                    className="flex w-full items-center justify-between gap-6 px-4 py-4 text-left text-body font-medium sm:px-5"
                  >
                    {t(q)}
                    <Plus
                      className={cx('size-5 shrink-0 text-biro transition-transform duration-300', expanded && 'rotate-45')}
                      aria-hidden="true"
                    />
                  </button>
                </h3>
                <div
                  id={`faq-a-${i}`}
                  role="region"
                  aria-labelledby={`faq-q-${i}`}
                  className={cx('faq-panel grid', expanded ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]')}
                >
                  <div className="overflow-hidden">
                    <p className={cx('max-w-[60ch] px-4 pb-5 text-base leading-[1.6] sm:px-5', !expanded && 'invisible')}>{t(a)}</p>
                  </div>
                </div>
              </li>
            )
          })}
        </ul>
      </div>
    </section>
  )
}

// ---------------------------------------------------------------- closing sentence

export function ClosingCall() {
  const t = useT()
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const create = useCreateRoom()

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return setError(t('Write a name in the blank first.'))
    setError(null)
    create.mutate({ name: name.trim() })
  }

  return (
    <section className="bg-surface px-4 py-24 lg:px-10 lg:py-40" aria-labelledby="cta-title">
      <form onSubmit={submit} className="mx-auto max-w-[1400px]" noValidate>
        <h2 id="cta-title" className="sr-only">
          {t('Start a room')}
        </h2>
        <p className="max-w-[22ch] text-heading leading-[1.35] font-medium" data-reveal>
          Save a seat for{' '}
          <label htmlFor="cta-name" className="sr-only">
            {t('Your name')}
          </label>
          <input
            id="cta-name"
            value={name}
            maxLength={32}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('your name')}
            className="mx-1 h-[1.35em] w-[7em] bg-[#e2e2e2] px-[0.3em] align-baseline text-ink placeholder:text-muted focus:outline-none focus-visible:outline-3 focus-visible:outline-biro"
          />{' '}
          and press{' '}
          <button
            type="submit"
            disabled={create.isPending}
            className="h-[1.35em] bg-biro px-[0.3em] align-baseline text-white transition-colors hover:bg-biro-deep disabled:opacity-60"
          >
            {create.isPending ? t('opening…') : t('play')}
          </button>
        </p>
        <p className="mt-6 text-base text-muted" role="status">
          {error ?? (create.error instanceof ApiError ? create.error.message : t('We open your room and hand you the invite link. No account, nothing to install.'))}
        </p>
      </form>
    </section>
  )
}

// ---------------------------------------------------------------- footer

export function Footer() {
  const t = useT()
  return (
    <footer className="flex min-h-svh flex-col bg-ink text-paper">
      <Stripe className="h-2" />
      <div className="grid flex-1 grid-rows-[auto_1fr_auto] px-4 pt-10 pb-5 lg:px-10">
        <div className="grid grid-cols-2 gap-8 text-base md:grid-cols-4">
          <FooterCol title={t('Watch')}>
            <a href="#top">{t('Create a room')}</a>
            <a href="#genres">{t('Pick a genre')}</a>
            <a href="#how">{t('How it works')}</a>
            <a href="#questions">{t('Questions')}</a>
          </FooterCol>
          <FooterCol title={t('Developers')}>
            <Link to="/developers">{t('API guide')}</Link>
            <Link to="/developers#key">{t('Get an API key')}</Link>
            <a href="/api/openapi.yaml">{t('OpenAPI spec')}</a>
          </FooterCol>
          <FooterCol title={t('Under the hood')}>
            <span>YouTube IFrame Player</span>
            <span>{t('WebRTC voice and camera')}</span>
            <span>{t('Rust and React')}</span>
          </FooterCol>
          <FooterCol title={t('Trailers shown')}>
            {[FEATURED.spongebob, FEATURED.wildwood, FEATURED.notAlone, FEATURED.forgottenIsland].map((v) => (
              <a key={v.id} href={`https://www.youtube.com/watch?v=${v.id}`}>
                {v.title.split('|')[0]!.trim()} · {v.channel}
              </a>
            ))}
            <span>{t('Thumbnails served by YouTube')}</span>
          </FooterCol>
        </div>
        <div />
        <div>
          <Perforation className="mb-6 text-paper/30" />
          <svg viewBox="0 0 620 120" className="block w-full" role="img" aria-label="Partello">
            <text x="-6" y="104" fill="currentColor" fontSize="138" fontWeight="600" letterSpacing="-6" fontFamily="Bricolage Grotesque Variable, sans-serif">
              Partello
            </text>
          </svg>
          <p className="pt-5 text-small text-[#a8a8a8]">
            {t("Partello is not affiliated with YouTube. Videos play in YouTube's official player and follow its terms. Trailer thumbnails belong to their owners.")}
          </p>
        </div>
      </div>
    </footer>
  )
}

function FooterCol({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2 [&>a]:text-[#c4c4c4] [&>a:hover]:text-paper [&>span]:text-[#c4c4c4]">
      <p className="font-medium text-paper">{title}</p>
      {children}
    </div>
  )
}
