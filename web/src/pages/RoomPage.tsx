import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import { ApiError, api } from '../lib/api'
import { storage, type StoredSession } from '../lib/storage'
import { RoomController } from '../room/controller'
import { RoomContext } from '../room/context'
import { CallProvider } from '../features/call/CallProvider'
import { useToast } from '../components/Toast'
import { Spinner } from '../components/Button'
import { Wordmark } from '../components/Wordmark'
import { JoinGate } from './JoinGate'
import { RoomLayout } from './RoomLayout'
import { useT } from '../i18n'

type Fatal = 'not_found' | 'room_full' | 'kicked' | 'too_many_tabs' | null

/** Reads `#host=<key>` (from the developer API's host_url) once and hides it from the address bar. */
function captureHostKey(slug: string) {
  const match = /(?:^|[#&])host=([A-Za-z0-9_-]+)/.exec(window.location.hash)
  if (match?.[1]) {
    storage.setHostKey(slug, match[1])
    history.replaceState(null, '', window.location.pathname + window.location.search)
  }
}

export function RoomPage() {
  const t = useT()
  const { slug = '' } = useParams()
  const navigate = useNavigate()
  const toast = useToast()
  const [session, setSession] = useState<StoredSession | null>(() => storage.getSession(slug))
  const [fatal, setFatal] = useState<Fatal>(null)
  const rejoining = useRef(false)

  useEffect(() => captureHostKey(slug), [slug])

  const summary = useQuery({
    queryKey: ['room', slug],
    queryFn: () => api.getRoom(slug),
    retry: (count, err) => !(err instanceof ApiError && err.status === 404) && count < 2,
  })

  const joinAs = useCallback(
    async (name: string, pin?: string) => {
      const res = await api.joinRoom(slug, name, storage.getHostKey(slug), pin)
      const next = { token: res.session_token, participantId: res.participant_id, displayName: name, pin }
      storage.setName(name)
      storage.setSession(slug, next)
      if (pin) storage.setRoomPin(slug, pin)
      setSession(next)
    },
    [slug],
  )

  // When the server no longer knows our session (expired, server restarted),
  // re-join silently with the remembered name.
  const recover = useCallback(async () => {
    const previous = storage.getSession(slug)
    const name = previous?.displayName ?? storage.getName()
    storage.clearSession(slug)
    setSession(null)
    if (!name || rejoining.current) return
    rejoining.current = true
    try {
      await joinAs(name, previous?.pin)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'room_not_found') setFatal('not_found')
      else if (err instanceof ApiError && err.code === 'room_full') setFatal('room_full')
    } finally {
      rejoining.current = false
    }
  }, [slug, joinAs])

  const controller = useMemo(() => (session ? new RoomController(slug, session.token) : null), [slug, session])

  useEffect(() => {
    controller?.setHandlers({
      onFatal: (reason) => {
        if (reason === 'unauthorized') void recover()
        else if (reason === 'not_found') setFatal('not_found')
        else if (reason === 'room_full') setFatal('room_full')
        else if (reason === 'kicked') {
          storage.clearSession(slug)
          setFatal('kicked')
        } else if (reason === 'too_many_tabs') setFatal('too_many_tabs')
        else if (reason === 'left') navigate('/')
      },
      onError: (message) => toast(message, 'error'),
    })
  }, [controller, recover, navigate, toast, slug])

  useEffect(() => {
    if (!controller) return
    controller.start()
    return () => controller.stop()
  }, [controller])

  const notFound = fatal === 'not_found' || (summary.error instanceof ApiError && summary.error.status === 404)
  if (notFound)
    return (
      <RoomMessage
        title={t("This room doesn't exist")}
        body={t('The link may be mistyped, or the room was closed after a long time without visitors.')}
      />
    )
  if (fatal === 'room_full') return <RoomMessage title={t('This room is full')} body={t('Ask the host to make space, or start your own room.')} />
  if (fatal === 'kicked')
    return <RoomMessage title={t('The host removed you from this room')} body={t('You can start your own room any time.')} />
  if (fatal === 'too_many_tabs')
    return (
      <RoomMessage
        title={t('This room is already open in other tabs')}
        body={t('Close one of the other tabs, then reload this page.')}
      />
    )

  if (!session) {
    return <JoinGate slug={slug} summary={summary.data} loading={summary.isPending} onJoin={joinAs} />
  }

  if (!controller) {
    return (
      <div className="room-scope grid min-h-dvh place-items-center bg-night text-fog">
        <Spinner className="size-6" />
      </div>
    )
  }

  return (
    <RoomContext.Provider value={controller}>
      <CallProvider>
        <RoomLayout
          onLeave={() => {
            controller.leave()
            storage.clearSession(slug)
            navigate('/')
          }}
        />
      </CallProvider>
    </RoomContext.Provider>
  )
}

function RoomMessage({ title, body }: { title: string; body: string }) {
  const t = useT()
  return (
    <main className="room-scope flex min-h-dvh flex-col bg-night px-4 py-6 text-fog sm:px-8">
      <Link to="/" className="text-xl">
        <Wordmark />
      </Link>
      <div className="flex flex-1 flex-col justify-center gap-4 py-16">
        <h1 className="display max-w-3xl text-head">{title}</h1>
        <p className="max-w-lg text-body text-haze">{body}</p>
        <Link to="/" className="mt-4 inline-flex h-12 w-fit items-center bg-biro px-5 font-medium text-white hover:bg-[#2c5cf0]">
          {t('Create a new room')}
        </Link>
      </div>
    </main>
  )
}
