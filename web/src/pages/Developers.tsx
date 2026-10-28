import { useState, type FormEvent, type ReactNode } from 'react'
import { Link } from 'react-router'
import { useMutation } from '@tanstack/react-query'
import { Check, Copy } from 'lucide-react'
import { api, ApiError } from '../lib/api'
import { Button } from '../components/Button'
import { Wordmark } from '../components/Wordmark'
import { Stripe } from './landing/Hero'
import { msg, useT } from '../i18n'
import { LanguageSwitch } from '../i18n/LanguageSwitch'

const ORIGIN = typeof window === 'undefined' ? '' : window.location.origin

const ENDPOINTS: { method: string; path: string; what: string; body?: string; reply: string }[] = [
  {
    method: 'POST',
    path: '/api/v1/rooms',
    what: msg('Create a room, optionally already playing a video.'),
    body: '{ "name": "Launch party", "video_url": "https://youtu.be/dQw4w9WgXcQ", "settings": { "control_mode": "everyone" } }',
    reply: '201 { room, invite_url, host_key, host_url }',
  },
  {
    method: 'GET',
    path: '/api/v1/rooms/{slug}',
    what: msg('What is playing, the queue, and how many people are in.'),
    reply: '200 { room, participant_count, now_playing, status, position, queue }',
  },
  {
    method: 'POST',
    path: '/api/v1/rooms/{slug}/play',
    what: msg('Replace the current video for everyone, right now.'),
    body: '{ "url": "https://www.youtube.com/watch?v=9bZkp7q19f0" }',
    reply: msg('200 room status'),
  },
  {
    method: 'POST',
    path: '/api/v1/rooms/{slug}/queue',
    what: msg('Append up to 50 videos to the queue.'),
    body: '{ "items": [{ "url": "https://youtu.be/kJQP7kiw5Fk" }] }',
    reply: msg('200 room status'),
  },
]

export function Developers() {
  const t = useT()
  return (
    <div className="min-h-dvh bg-paper text-ink">
      <header className="flex h-14 items-center gap-6 bg-ink px-4 text-paper lg:px-6">
        <Link to="/" className="text-lg" aria-label="Partello home">
          <Wordmark />
        </Link>
        <nav className="ml-auto flex items-center gap-5 text-[0.9375rem]">
          <a href="#key" className="hover:underline">
            {t('Get a key')}
          </a>
          <a href="/api/openapi.yaml" className="hidden hover:underline sm:inline">
            OpenAPI
          </a>
          <LanguageSwitch />
        </nav>
      </header>
      <main className="mx-auto grid max-w-[1400px] gap-16 px-4 py-20 lg:grid-cols-12 lg:px-10 lg:py-28">
        <div className="lg:col-span-7">
          <h1 className="display text-head">{t('The Partello API')}</h1>
          <p className="mt-8 max-w-[52ch] text-body">
            {t(
              'Open watch-party rooms from your own product. Your server creates the room and picks the video; your users get a link to a Partello room that is already playing, with chat and calls built in.',
            )}
          </p>
          <p className="mt-4 max-w-[52ch] text-body">
            {t('Keys are server-side secrets. Send them as')} <code className="bg-surface px-1">Authorization: Bearer ptk_…</code>{' '}
            {t('from your backend, never from browser code. Each key can manage only the rooms it created and makes up to 60 calls a minute.')}
          </p>

          <h2 className="mt-16 text-lead font-semibold">{t('Endpoints')}</h2>
          <ul className="mt-6 flex flex-col gap-4">
            {ENDPOINTS.map((e) => (
              <li key={e.method + e.path} className="bg-surface p-5">
                <p className="flex flex-wrap items-baseline gap-3">
                  <span className="bg-ink px-2 py-0.5 text-[0.9375rem] font-semibold text-paper">{e.method}</span>
                  <code className="text-[0.9375rem] font-medium">{e.path}</code>
                </p>
                <p className="mt-3">{t(e.what)}</p>
                {e.body && <Code>{e.body}</Code>}
                <p className="mt-3 text-[0.9375rem] text-muted">{t('Returns {reply}', { reply: t(e.reply) })}</p>
              </li>
            ))}
          </ul>

          <h2 className="mt-16 text-lead font-semibold">{t('Hosting a room you created')}</h2>
          <p className="mt-4 max-w-[52ch]">
            <code className="bg-surface px-1">host_url</code> {t("carries the room's host key after")}{' '}
            <code className="bg-surface px-1">#host=</code>. {t('Whoever opens it becomes the host. Give it only to the person who should run the room; share')}{' '}
            <code className="bg-surface px-1">invite_url</code> {t('with everyone else.')}
          </p>

          <h2 className="mt-16 text-lead font-semibold">{t('Errors')}</h2>
          <p className="mt-4 max-w-[52ch]">
            {t('Errors are JSON:')} <code className="bg-surface px-1">{'{ "error": { "code", "message" } }'}</code>. {t('Codes include')}{' '}
            <code>unauthorized</code>, <code>room_not_found</code>, <code>invalid_video</code>, <code>validation_failed</code>,{' '}
            <code>rate_limited</code>. {t('The full schema is in the')}{' '}
            <a className="text-biro underline" href="/api/openapi.yaml">
              {t('OpenAPI document')}
            </a>
            .
          </p>
        </div>
        <aside className="lg:col-span-4 lg:col-start-9">
          <KeyForm />
        </aside>
      </main>
    </div>
  )
}

function Code({ children }: { children: ReactNode }) {
  return (
    <pre className="mt-3 overflow-x-auto bg-ink p-4 text-[0.875rem] leading-relaxed text-paper">
      <code>{children}</code>
    </pre>
  )
}

function KeyForm() {
  const t = useT()
  const [label, setLabel] = useState('')
  const [copied, setCopied] = useState(false)
  const create = useMutation({ mutationFn: (l: string) => api.createApiKey(l) })

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (label.trim()) create.mutate(label.trim())
  }

  const key = create.data?.api_key
  return (
    <div id="key" className="sticky top-6 bg-ink text-paper">
      <Stripe className="h-8 [&>span:nth-child(even)]:bg-ink [&>span:nth-child(odd)]:bg-paper" />
      <div className="p-6">
        <h2 className="text-lead font-semibold">{t('Get an API key')}</h2>
        {key ? (
          <div className="mt-5 flex flex-col gap-4">
            <p>{t('Copy it now. For your safety it is shown only once and Partello keeps just a fingerprint of it.')}</p>
            <code className="block break-all bg-paper p-3 text-[0.9375rem] text-ink">{key}</code>
            <Button
              tone="dark"
              variant="ink"
              icon={copied ? <Check className="size-4" /> : <Copy className="size-4" />}
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(key)
                  setCopied(true)
                } catch {
                  // Clipboard blocked: the key is still selectable on screen.
                  setCopied(false)
                }
              }}
            >
              {copied ? t('Copied') : t('Copy key')}
            </Button>
            <Code>{`curl -X POST ${ORIGIN}/api/v1/rooms \\
  -H "Authorization: Bearer ${key.slice(0, 10)}…" \\
  -H "Content-Type: application/json" \\
  -d '{"name":"Movie night"}'`}</Code>
          </div>
        ) : (
          <form onSubmit={submit} className="mt-5 flex flex-col gap-3" noValidate>
            <label htmlFor="key-label">{t('What is it for?')}</label>
            <input
              id="key-label"
              value={label}
              maxLength={60}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t('My Discord bot')}
              className="h-12 bg-paper px-3 text-ink placeholder:text-muted focus:outline-none focus-visible:outline-3 focus-visible:outline-biro-bright"
            />
            {create.error && (
              <p role="alert" className="text-[0.9375rem] text-[#ffb4b4]">
                {create.error instanceof ApiError ? create.error.message : t('Could not create a key')}
              </p>
            )}
            <Button type="submit" tone="dark" loading={create.isPending} disabled={!label.trim()}>
              {t('Create key')}
            </Button>
            <p className="text-small text-[#c4c4c4]">{t('Five keys per hour per network. Free during the beta.')}</p>
          </form>
        )}
      </div>
    </div>
  )
}
