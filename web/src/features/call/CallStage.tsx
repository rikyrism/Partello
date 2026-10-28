import { memo, useEffect, useRef, useState } from 'react'
import { AudioLines, Camera, CameraOff, Hand, Loader2, Mic, MicOff, PhoneOff, Phone, Volume1, VolumeX } from 'lucide-react'
import { Avatar } from '../../components/Avatar'
import { Button, IconButton } from '../../components/Button'
import type { Participant } from '../../realtime/protocol'
import { useRoom } from '../../room/context'
import { cx } from '../../lib/format'
import { useT } from '../../i18n'
import { useCall } from './CallProvider'
import { useSpeaking } from './speaking'

/** Whether the call strip has anything to show. */
export function useCallStageShown() {
  const call = useCall()
  const anyoneInCall = useRoom((s) => s.participants.some((p) => p.voice))
  return anyoneInCall || call.status !== 'idle'
}

/**
 * Camera/avatar tiles for everyone in the call, as one row that scrolls
 * sideways so the video above never moves. Renders nothing when the call is
 * empty. Hide it with CSS (className), never by unmounting: it also plays
 * everyone's audio.
 */
export function CallStage({ className }: { className?: string }) {
  const call = useCall()
  const selfId = useRoom((s) => s.selfId)
  const participants = useRoom((s) => s.participants)
  const inCall = participants.filter((p) => p.voice)
  if (inCall.length === 0 && call.status === 'idle') return null

  // Self first, then by join order.
  const ordered = [...inCall].sort((a, b) => (a.id === selfId ? -1 : b.id === selfId ? 1 : a.joined_at - b.joined_at))

  return (
    <section aria-label="Call" className={cx('shrink-0 bg-night-2 px-2 pb-2', className)}>
      <ul className="scrollbar-thin flex gap-2 overflow-x-auto">
        {ordered.map((p) => {
          const isSelf = p.id === selfId
          return (
            <li key={p.id} className="w-36 shrink-0 sm:w-44 lg:w-48">
              <Tile
                participant={p}
                isSelf={isSelf}
                stream={isSelf ? call.localStream : call.remoteStreams[p.id]}
                connecting={!isSelf && call.status === 'joined' && call.peerStates[p.id] !== 'connected'}
                listening={call.status === 'joined'}
              />
            </li>
          )
        })}
      </ul>
      <RemoteAudio />
    </section>
  )
}

const Tile = memo(function Tile({
  participant,
  isSelf,
  stream,
  connecting,
  listening,
}: {
  participant: Participant
  isSelf: boolean
  stream: MediaStream | null | undefined
  connecting: boolean
  listening: boolean
}) {
  const t = useT()
  const videoRef = useRef<HTMLVideoElement>(null)
  const voice = participant.voice
  const showVideo = !!voice?.camera && !!stream?.getVideoTracks().length
  const speaking = useSpeaking(stream, listening && !voice?.muted)

  useEffect(() => {
    const el = videoRef.current
    if (el && el.srcObject !== (stream ?? null)) el.srcObject = stream ?? null
  }, [stream, showVideo])

  const label = isSelf ? t('{name} (you)', { name: participant.display_name }) : participant.display_name
  const state = [voice?.muted ? t('muted') : null, speaking ? t('speaking') : null].filter(Boolean).join(', ')
  return (
    <figure className={cx('relative m-0 aspect-video overflow-hidden bg-night-3', speaking && 'is-speaking')} aria-label={state ? `${label}, ${state}` : label}>
      {showVideo ? (
        <video ref={videoRef} autoPlay playsInline muted className={cx('size-full object-cover', isSelf && '-scale-x-100')} />
      ) : (
        <div className="grid size-full place-items-center">
          <Avatar id={participant.id} name={participant.display_name} size="lg" className="size-12 text-lg sm:size-14" />
        </div>
      )}
      <figcaption className="absolute inset-x-0 bottom-0 flex items-center gap-1.5 bg-black/55 px-2 py-1 text-small text-fog">
        <span className="min-w-0 flex-1 truncate">{label}</span>
        {connecting && <Loader2 className="size-3.5 animate-spin" aria-label={t('Connecting')} />}
        {!voice?.camera && <CameraOff className="size-3.5 text-haze" aria-label={t('Camera off')} />}
        {voice?.muted ? <MicOff className="size-3.5 text-alarm" aria-label={t('Muted')} /> : <Mic className="size-3.5" aria-hidden="true" />}
      </figcaption>
    </figure>
  )
})

/** One <audio> per remote participant so each has its own volume. */
function RemoteAudio() {
  const { remoteStreams, volumes } = useCall()
  return (
    <>
      {Object.entries(remoteStreams).map(([id, stream]) => (
        <PeerAudio key={id} stream={stream} volume={volumes[id] ?? 1} />
      ))}
    </>
  )
}

function PeerAudio({ stream, volume }: { stream: MediaStream; volume: number }) {
  const ref = useRef<HTMLAudioElement>(null)
  useEffect(() => {
    if (ref.current && ref.current.srcObject !== stream) ref.current.srcObject = stream
  }, [stream])
  useEffect(() => {
    if (ref.current) ref.current.volume = volume
  }, [volume])
  return <audio ref={ref} autoPlay />
}

/** Join/leave, mic, camera, push-to-talk and video ducking. */
export function CallControls({ compact = false }: { compact?: boolean }) {
  const t = useT()
  const call = useCall()
  const connected = useRoom((s) => s.status === 'open')
  const inCallCount = useRoom((s) => s.participants.filter((p) => p.voice).length)
  const maxVoice = useRoom((s) => s.limits?.max_voice_participants ?? 8)
  const full = call.status === 'idle' && inCallCount >= maxVoice
  const size = compact ? 'sm' : 'md'

  if (call.status !== 'joined') {
    return (
      <div className="flex items-center gap-2">
        <Button
          tone="dark"
          variant="primary"
          size={size}
          loading={call.status === 'joining'}
          disabled={!connected || full}
          icon={<Phone className="size-4" />}
          onClick={() => void call.join(false)}
          title={full ? t('The call is full') : undefined}
        >
          {call.status === 'joining' ? t('Joining…') : inCallCount > 0 ? t('Join call ({n})', { n: inCallCount }) : t('Join call')}
        </Button>
        <IconButton label={t('Join call with camera')} size={size} disabled={!connected || full || call.status === 'joining'} onClick={() => void call.join(true)}>
          <Camera className="size-[18px]" />
        </IconButton>
      </div>
    )
  }

  const talking = call.pushToTalk && call.talkHeld
  return (
    <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t('Call controls')}>
      {call.pushToTalk ? (
        <button
          type="button"
          className={cx(
            'inline-flex h-9 select-none items-center gap-2 px-3 text-[0.9375rem] font-medium transition-colors touch-none',
            talking ? 'bg-biro text-white' : 'bg-night-3 text-fog hover:bg-night-4',
          )}
          aria-pressed={talking}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId)
            call.setTalkHeld(true)
          }}
          onPointerUp={() => call.setTalkHeld(false)}
          onPointerCancel={() => call.setTalkHeld(false)}
          onKeyDown={(e) => {
            if ((e.key === 'Enter' || e.key === ' ') && !e.repeat) call.setTalkHeld(true)
          }}
          onKeyUp={() => call.setTalkHeld(false)}
        >
          <Mic className="size-4" aria-hidden="true" />
          {talking ? t('Talking…') : t('Hold to talk')}
          <span className="hidden text-small text-haze lg:inline">{t('or hold Space')}</span>
        </button>
      ) : (
        <IconButton size={size} label={call.muted ? t('Unmute microphone') : t('Mute microphone')} active={!call.muted} onClick={call.toggleMute} aria-pressed={!call.muted}>
          {call.muted ? <MicOff className="size-[18px]" /> : <Mic className="size-[18px]" />}
        </IconButton>
      )}
      <IconButton
        size={size}
        label={call.camera ? t('Turn camera off') : t('Turn camera on')}
        active={call.camera}
        aria-pressed={call.camera}
        disabled={call.cameraBusy}
        onClick={() => void call.toggleCamera()}
      >
        {call.cameraBusy ? <Loader2 className="size-[18px] animate-spin" /> : call.camera ? <Camera className="size-[18px]" /> : <CameraOff className="size-[18px]" />}
      </IconButton>
      <IconButton
        size={size}
        label={call.pushToTalk ? t('Turn off push-to-talk') : t('Turn on push-to-talk')}
        title={t('Push-to-talk: your microphone only sends while you hold the button or Space')}
        active={call.pushToTalk}
        aria-pressed={call.pushToTalk}
        onClick={() => call.setPushToTalk(!call.pushToTalk)}
      >
        <Hand className="size-[18px]" />
      </IconButton>
      <IconButton
        size={size}
        label={call.autoDuck ? t('Stop lowering the video when people talk') : t('Lower the video when people talk')}
        title={t('Lowers the video while anyone talks, so voices stay clear')}
        active={call.autoDuck}
        aria-pressed={call.autoDuck}
        onClick={() => call.setAutoDuck(!call.autoDuck)}
      >
        <AudioLines className="size-[18px]" />
      </IconButton>
      <Button tone="dark" variant="danger" size={size} icon={<PhoneOff className="size-4" />} onClick={call.leave}>
        {compact ? <span className="sr-only">{t('Leave call')}</span> : t('Leave call')}
      </Button>
    </div>
  )
}

/** Per-person volume, shown in the people list while in a call. */
export function VolumeControl({ peerId, name }: { peerId: string; name: string }) {
  const t = useT()
  const call = useCall()
  const [open, setOpen] = useState(false)
  if (call.status !== 'joined' || !call.remoteStreams[peerId]) return null
  const volume = call.volumes[peerId] ?? 1
  const label = t('Volume for {name}', { name })
  return (
    <div className="flex items-center gap-1">
      <IconButton size="sm" label={label} onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        {volume === 0 ? <VolumeX className="size-4" /> : <Volume1 className="size-4" />}
      </IconButton>
      {open && (
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={volume}
          aria-label={label}
          onChange={(e) => call.setVolume(peerId, Number(e.target.value))}
          className="w-24 accent-biro-bright"
        />
      )}
    </div>
  )
}
