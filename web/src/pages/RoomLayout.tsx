import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { Link } from 'react-router'
import { Link2, Lock, LogOut, MessageSquare, ListVideo, PanelRightClose, PanelRightOpen, Settings, Users } from 'lucide-react'
import { useController, useRoom, useSelf } from '../room/context'
import { SyncedPlayer } from '../features/player/SyncedPlayer'
import { ChatPanel } from '../features/chat/ChatPanel'
import { QueuePanel } from '../features/queue/QueuePanel'
import { PeoplePanel } from '../features/people/PeoplePanel'
import { CallControls, CallStage, useCallStageShown } from '../features/call/CallStage'
import { ReactionBar } from '../features/reactions/Reactions'
import { TheaterOverlay } from '../features/theater/TheaterOverlay'
import { NotifyButton } from '../features/notify/NotifyButton'
import { useChatNotifications } from '../features/chat/useChatNotifications'
import { useCall } from '../features/call/CallProvider'
import { Button, IconButton } from '../components/Button'
import { useToast } from '../components/Toast'
import { Wordmark } from '../components/Wordmark'
import { api, ApiError } from '../lib/api'
import { cx } from '../lib/format'
import { storage } from '../lib/storage'
import { useMedia, useSoftKeyboard } from '../lib/viewport'
import { InviteDialog } from '../features/invite/InviteDialog'
import type { ControlMode, QueueMode } from '../realtime/protocol'
import { translate, useT } from '../i18n'
import { LanguageSwitch } from '../i18n/LanguageSwitch'

type Tab = 'chat' | 'queue' | 'people'

export function RoomLayout({ onLeave }: { onLeave: () => void }) {
  const t = useT()
  const room = useRoom((s) => s.room)
  const call = useCall()
  const [tab, setTab] = useState<Tab>('chat')
  // Two columns from 1024px (laptops, iPad landscape); one column below,
  // where the video stays put and only the panel underneath scrolls.
  const wide = useMedia('(min-width: 1024px)')
  const [sideOpen, setSideOpen] = useState(true)
  const sideShown = !wide || sideOpen
  const unread = useUnread(tab === 'chat' && sideShown)
  useChatNotifications(tab === 'chat' && sideShown)
  const queueLen = useRoom((s) => s.queue.length)
  const hasVideo = useRoom((s) => !!s.playback?.item && s.playback.status !== 'idle')
  // A phone on its side has no room for anything but the video.
  const phoneLandscape = useMedia('(orientation: landscape) and (max-height: 500px) and (pointer: coarse)')
  const [theaterRef, theaterOn, toggleTheater] = useTheater(phoneLandscape && hasVideo)
  const people = useRoom((s) => s.participants.length)
  const stageShown = useCallStageShown()
  const keyboard = useSoftKeyboard()
  // Typing in one column: keep the video in sight, smaller, and drop the rest.
  const compact = keyboard != null && !wide

  useEffect(() => {
    document.title = room ? `${room.name} · Partello` : 'Partello'
    return () => {
      document.title = translate('Partello — Watch together. Wherever.')
    }
  }, [room])

  const leave = () => {
    call.leave()
    onLeave()
  }

  return (
    <div className="room-scope flex h-dvh flex-col overflow-hidden bg-night text-fog" style={keyboard != null ? { height: keyboard } : undefined}>
      <RoomHeader onLeave={leave} side={wide ? { open: sideOpen, unread, toggle: () => setSideOpen((o) => !o) } : undefined} />
      <div
        className={cx(
          'flex min-h-0 flex-1 flex-col overflow-y-auto lg:grid lg:overflow-hidden',
          sideShown && 'lg:grid-cols-[minmax(0,1fr)_380px] xl:grid-cols-[minmax(0,1fr)_420px]',
        )}
      >
        <main className="flex shrink-0 flex-col lg:min-h-0 lg:shrink lg:overflow-y-auto lg:scrollbar-thin">
          <div
            ref={theaterRef}
            className={cx(theaterOn ? 'fixed inset-0 z-50 flex flex-col bg-black' : 'bg-night')}
            onKeyDown={(e) => e.key === 'Escape' && theaterOn && toggleTheater()}
          >
            {/* Two columns: the player, its controls and the call strip all fit one screen. */}
            <div
              className={cx(
                theaterOn
                  ? 'flex min-h-0 w-full flex-1 flex-col'
                  : cx('mx-auto w-full', stageShown ? 'lg:max-w-[calc((100dvh-19rem)*16/9)]' : 'lg:max-w-[calc((100dvh-11.5rem)*16/9)]'),
              )}
              style={compact && !theaterOn ? { maxWidth: Math.round(keyboard * 0.32 * (16 / 9)) } : undefined}
            >
              <SyncedPlayer
                fill={theaterOn}
                theater={theaterOn}
                onToggleTheater={toggleTheater}
                overlay={theaterOn ? <TheaterOverlay /> : null}
              />
            </div>
          </div>
          <div className={cx('flex flex-wrap items-center justify-between gap-2 bg-night-2 px-2 py-2 sm:px-3', compact && 'hidden')}>
            <ReactionBar className="w-full justify-between sm:w-auto sm:justify-start" />
            <p className="hidden min-w-0 flex-1 text-small text-haze md:block">
              {call.status === 'joined'
                ? t("Use headphones so the video doesn't leak into your microphone.")
                : t('Join the call to talk while you watch.')}
            </p>
            <CallControls compact />
          </div>
          <CallStage className={compact ? 'hidden' : undefined} />
        </main>

        <aside className={cx('flex min-h-48 flex-1 flex-col bg-night-2 lg:min-h-0', !sideShown && 'hidden')} aria-label={t('Room panels')}>
          <div role="tablist" aria-label={t('Room panels')} className="flex shrink-0 bg-night">
            <TabButton id="chat" current={tab} onSelect={setTab} icon={<MessageSquare className="size-4" />} badge={unread}>
              {t('Chat')}
            </TabButton>
            <TabButton id="queue" current={tab} onSelect={setTab} icon={<ListVideo className="size-4" />} count={queueLen}>
              {t('Queue')}
            </TabButton>
            <TabButton id="people" current={tab} onSelect={setTab} icon={<Users className="size-4" />} count={people}>
              {t('People')}
            </TabButton>
          </div>
          <TabPanel id="chat" current={tab}>
            <ChatPanel />
          </TabPanel>
          <TabPanel id="queue" current={tab}>
            <QueuePanel />
          </TabPanel>
          <TabPanel id="people" current={tab}>
            <PeoplePanel />
          </TabPanel>
        </aside>
      </div>
    </div>
  )
}

/**
 * Theater mode: the player area goes full screen with chat and reactions
 * over the video. Uses the Fullscreen API where it exists, and a fixed
 * full-window layout where it doesn't (iPhone). `auto` turns it on without a
 * tap (a phone held sideways), so that case uses the full-window layout only.
 */
function useTheater(auto: boolean) {
  const ref = useRef<HTMLDivElement>(null)
  const [manual, setManual] = useState(false)
  // Leaving an automatic theater sticks until the phone turns upright again.
  const [dismissed, setDismissed] = useState(false)
  const [wasAuto, setWasAuto] = useState(auto)
  if (wasAuto !== auto) {
    setWasAuto(auto)
    if (!auto) setDismissed(false)
  }
  const on = manual || (auto && !dismissed)

  useEffect(() => {
    const onChange = () => {
      if (!document.fullscreenElement) setManual(false)
    }
    document.addEventListener('fullscreenchange', onChange)
    return () => document.removeEventListener('fullscreenchange', onChange)
  }, [])

  const toggle = useCallback(() => {
    if (on) {
      setManual(false)
      if (auto) setDismissed(true)
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined)
      return
    }
    setManual(true)
    const el = ref.current
    if (el?.requestFullscreen) void el.requestFullscreen({ navigationUI: 'hide' }).catch(() => undefined)
  }, [on, auto])

  return [ref, on, toggle] as const
}

/** Messages that arrived while the chat tab was hidden. */
function useUnread(chatVisible: boolean): number {
  const count = useRoom((s) => s.chat.reduce((n, e) => (e.kind === 'message' ? n + 1 : n), 0))
  const [seen, setSeen] = useState(count)
  if (chatVisible && seen !== count) setSeen(count)
  return chatVisible ? 0 : Math.max(0, count - seen)
}

function TabButton({
  id,
  current,
  onSelect,
  icon,
  badge,
  count,
  children,
}: {
  id: Tab
  current: Tab
  onSelect: (t: Tab) => void
  icon: ReactNode
  badge?: number
  count?: number
  children: ReactNode
}) {
  const t = useT()
  const selected = id === current
  return (
    <button
      role="tab"
      id={`tab-${id}`}
      aria-selected={selected}
      aria-controls={`panel-${id}`}
      tabIndex={selected ? 0 : -1}
      onClick={() => onSelect(id)}
      onKeyDown={(e) => {
        const order: Tab[] = ['chat', 'queue', 'people']
        const i = order.indexOf(id)
        const next = e.key === 'ArrowRight' ? order[(i + 1) % 3] : e.key === 'ArrowLeft' ? order[(i + 2) % 3] : null
        if (next) {
          onSelect(next)
          document.getElementById(`tab-${next}`)?.focus()
        }
      }}
      className={cx(
        'relative flex h-12 flex-1 items-center justify-center gap-2 text-[0.9375rem] font-medium transition-colors',
        selected ? 'bg-night-2 text-fog' : 'text-haze hover:text-fog',
      )}
    >
      {icon}
      {children}
      {!!count && <span className="tabular text-small text-haze">{count}</span>}
      {!!badge && (
        <span className="tabular min-w-5 bg-biro px-1.5 text-xs leading-5 text-white" aria-label={t('{n} unread', { n: badge })}>
          {badge > 99 ? '99+' : badge}
        </span>
      )}
    </button>
  )
}

function TabPanel({ id, current, children }: { id: Tab; current: Tab; children: ReactNode }) {
  return (
    <div role="tabpanel" id={`panel-${id}`} aria-labelledby={`tab-${id}`} hidden={id !== current} className="min-h-0 flex-1">
      {children}
    </div>
  )
}

interface SideToggle {
  open: boolean
  unread: number
  toggle: () => void
}

function RoomHeader({ onLeave, side }: { onLeave: () => void; side?: SideToggle }) {
  const t = useT()
  const room = useRoom((s) => s.room)
  const self = useSelf()
  const [settingsOpen, setSettingsOpen] = useState(false)

  return (
    <header className="flex h-14 shrink-0 items-center gap-2 px-2 sm:gap-3 sm:px-4">
      <Link to="/" className="shrink-0 text-lg" aria-label="Partello home">
        <Wordmark mark className="[&>span:last-child]:hidden sm:[&>span:last-child]:inline" />
      </Link>
      <div className="flex min-w-0 flex-1 items-center gap-2.5">
        <h1 className="truncate font-medium">{room?.name ?? t('Loading room…')}</h1>
        {room?.has_password && <Lock className="size-3.5 shrink-0 text-haze" aria-label={t('Protected with a PIN')} />}
        <ConnectionBadge />
      </div>
      <LanguageSwitch className="hidden sm:flex" />
      <InviteButton />
      <NotifyButton />
      {self?.is_host && (
        <IconButton label={t('Room settings')} onClick={() => setSettingsOpen(true)}>
          <Settings className="size-[18px]" />
        </IconButton>
      )}
      {side && (
        <IconButton label={side.open ? t('Hide side panel') : t('Show side panel')} onClick={side.toggle} aria-expanded={side.open} className="relative">
          {side.open ? <PanelRightClose className="size-[18px]" /> : <PanelRightOpen className="size-[18px]" />}
          {!side.open && side.unread > 0 && (
            <span className="absolute top-1.5 right-1.5 size-2 rounded-full bg-biro-bright" aria-label={t('{n} unread', { n: side.unread })} />
          )}
        </IconButton>
      )}
      <IconButton label={t('Leave room')} onClick={onLeave}>
        <LogOut className="size-[18px]" />
      </IconButton>
      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
    </header>
  )
}

function ConnectionBadge() {
  const t = useT()
  const status = useRoom((s) => s.status)
  const label = t(status === 'open' ? 'Live' : status === 'closed' ? 'Offline' : status === 'connecting' ? 'Connecting' : 'Reconnecting')
  return (
    <span className="inline-flex shrink-0 items-center gap-1.5 text-small text-haze" role="status" aria-label={t('Connection: {state}', { state: label })}>
      <span
        className={cx(
          'size-2 rounded-full',
          status === 'open' ? 'bg-biro-bright' : status === 'closed' ? 'bg-alarm' : 'animate-pulse bg-haze',
        )}
        aria-hidden="true"
      />
      <span className="hidden sm:inline">{label}</span>
    </span>
  )
}

function InviteButton() {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button tone="dark" variant="primary" size="sm" onClick={() => setOpen(true)} icon={<Link2 className="size-4" />} aria-haspopup="dialog">
        {t('Invite')}
      </Button>
      {open && <InviteDialog onClose={() => setOpen(false)} />}
    </>
  )
}

function SettingsDialog({ onClose }: { onClose: () => void }) {
  const t = useT()
  const ctrl = useController()
  const room = useRoom((s) => s.room)
  const toast = useToast()
  const ref = useRef<HTMLDialogElement>(null)
  const [name, setName] = useState(room?.name ?? '')
  const [control, setControl] = useState<ControlMode>(room?.settings.control_mode ?? 'host_only')
  const [queue, setQueue] = useState<QueueMode>(room?.settings.queue_mode ?? 'everyone')
  const [permanent, setPermanent] = useState(!!room?.settings.permanent)
  const [pin, setPin] = useState('')
  const [removePin, setRemovePin] = useState(false)
  const hasPin = !!room?.has_password
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const d = ref.current
    d?.showModal()
    return () => d?.close()
  }, [])

  const save = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const pinPatch = removePin ? '' : pin || undefined
      if (pinPatch && !/^\d{4}$/.test(pinPatch)) {
        setError(t('PIN must be exactly 4 digits'))
        setBusy(false)
        return
      }
      await api.updateSettings(ctrl.slug, ctrl.token, {
        name: name.trim(),
        control_mode: control,
        queue_mode: queue,
        permanent,
        pin: pinPatch,
      })
      // Remember the PIN on this device so invitations can include it.
      if (pinPatch !== undefined) storage.setRoomPin(ctrl.slug, pinPatch || null)
      toast(t('Room settings saved'))
      onClose()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : t('Could not save settings'))
      setBusy(false)
    }
  }

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      className="m-auto w-[min(92vw,30rem)] bg-night-2 p-0 text-fog shadow-[0_24px_60px_rgba(0,0,0,0.6)] backdrop:bg-black/70" /* taste-skill-ignore: a real modal dialog */
      aria-labelledby="settings-title"
    >
      <form onSubmit={save} className="flex flex-col gap-6 p-6">
        <h2 id="settings-title" className="text-lead font-semibold">
          {t('Room settings')}
        </h2>
        <label className="flex flex-col gap-2">
          <span className="font-medium">{t('Room name')}</span>
          <input
            value={name}
            maxLength={60}
            onChange={(e) => setName(e.target.value)}
            className="h-11 bg-night-3 px-3 text-fog focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
          />
        </label>
        <Choice
          legend={t('Who can play, pause and skip')}
          value={control}
          onChange={(v) => setControl(v as ControlMode)}
          options={[
            ['host_only', t('Only the host')],
            ['everyone', t('Everyone')],
          ]}
        />
        <Choice
          legend={t('Who can add videos')}
          value={queue}
          onChange={(v) => setQueue(v as QueueMode)}
          options={[
            ['everyone', t('Everyone')],
            ['host_only', t('Only the host')],
          ]}
        />
        <div className="flex flex-col gap-1.5">
          <label className="flex items-center gap-2.5 font-medium">
            <input type="checkbox" checked={permanent} onChange={(e) => setPermanent(e.target.checked)} className="size-4 accent-biro-bright" />
            {t('Keep this room forever')}
          </label>
          <p className="pl-6.5 text-small text-haze">
            {permanent
              ? t('The link keeps working, so your group can come back to the same room every time.')
              : t('Rooms nobody visits for 30 days are deleted. Keep it to reuse the same link.')}
          </p>
        </div>
        <div className="flex flex-col gap-2">
          <label htmlFor="room-pin-setting" className="font-medium">
            {hasPin ? t('Change the PIN') : t('Room PIN (optional)')}
          </label>
          <input
            id="room-pin-setting"
            type="password"
            inputMode="numeric"
            autoComplete="off"
            pattern="[0-9]{4}"
            maxLength={4}
            value={pin}
            disabled={removePin}
            onChange={(e) => setPin(e.target.value.replace(/\D/g, '').slice(0, 4))}
            placeholder="••••"
            aria-describedby="pin-setting-hint"
            className="tabular h-11 w-32 bg-night-3 px-3 text-center text-lg tracking-[0.5em] text-fog placeholder:text-haze focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright disabled:opacity-50"
          />
          {hasPin && (
            <label className="flex items-center gap-2 text-small text-haze">
              <input type="checkbox" checked={removePin} onChange={(e) => setRemovePin(e.target.checked)} />
              {t('Remove the PIN')}
            </label>
          )}
          <p id="pin-setting-hint" className="text-small text-haze">
            {hasPin
              ? t('Leave empty to keep the current PIN. People already here stay; you never need it yourself.')
              : t('4 digits. Without a PIN, anyone with the link can join.')}
          </p>
        </div>
        {error && (
          <p role="alert" className="text-small text-alarm">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" tone="dark" variant="secondary" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button type="submit" tone="dark" loading={busy}>
            {t('Save')}
          </Button>
        </div>
      </form>
    </dialog>
  )
}

function Choice({ legend, value, onChange, options }: { legend: string; value: string; onChange: (v: string) => void; options: [string, string][] }) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="pb-2 font-medium">{legend}</legend>
      <div className="flex gap-2">
        {options.map(([v, label]) => (
          <label
            key={v}
            className={cx(
              'flex h-11 flex-1 cursor-pointer items-center justify-center text-[0.9375rem] has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-biro-bright',
              value === v ? 'bg-fog text-night' : 'bg-night-3 text-fog hover:bg-night-4',
            )}
          >
            <input type="radio" className="sr-only" name={legend} value={v} checked={value === v} onChange={() => onChange(v)} />
            {label}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
