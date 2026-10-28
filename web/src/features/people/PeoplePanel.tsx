import { useState } from 'react'
import { Camera, Crown, Mic, MicOff, UserX, WifiOff } from 'lucide-react'
import { Avatar } from '../../components/Avatar'
import { Button, IconButton } from '../../components/Button'
import { useController, useRoom, useSelf } from '../../room/context'
import type { Participant } from '../../realtime/protocol'
import { VolumeControl } from '../call/CallStage'
import { useT } from '../../i18n'

export function PeoplePanel() {
  const t = useT()
  const participants = useRoom((s) => s.participants)
  const max = useRoom((s) => s.room?.max_participants)
  const synced = useRoom((s) => s.synced)
  const self = useSelf()
  const sorted = [...participants].sort((a, b) => Number(b.is_host) - Number(a.is_host) || a.joined_at - b.joined_at)

  return (
    <div className="scrollbar-thin h-full overflow-y-auto px-3 py-3">
      <p className="pb-3 text-small text-haze">
        {t('{n} of {max} people here', { n: participants.length, max: max ?? '–' })}
      </p>
      {!synced ? (
        <div className="h-12 animate-pulse bg-night-3" aria-label={t('Loading people')} />
      ) : (
        <ul className="flex flex-col gap-1">
          {sorted.map((p) => (
            <PersonRow key={p.id} person={p} isSelf={p.id === self?.id} viewerIsHost={!!self?.is_host} />
          ))}
        </ul>
      )}
    </div>
  )
}

function PersonRow({ person, isSelf, viewerIsHost }: { person: Participant; isSelf: boolean; viewerIsHost: boolean }) {
  const t = useT()
  const ctrl = useController()
  const [confirmKick, setConfirmKick] = useState(false)
  const v = person.voice
  return (
    <li className="group flex min-h-14 items-center gap-3 px-1 py-1.5">
      <Avatar id={person.id} name={person.display_name} />
      <div className="min-w-0 flex-1">
        <p className="flex items-center gap-1.5 truncate font-medium text-fog">
          <span className="truncate">{person.display_name}</span>
          {isSelf && <span className="text-haze">{t('(you)')}</span>}
        </p>
        <p className="flex flex-wrap items-center gap-x-2.5 text-small text-haze">
          {person.is_host && (
            <span className="inline-flex items-center gap-1 text-biro-bright">
              <Crown className="size-3.5" aria-hidden="true" /> {t('Host')}
            </span>
          )}
          {!person.connected && (
            <span className="inline-flex items-center gap-1">
              <WifiOff className="size-3.5" aria-hidden="true" /> {t('Reconnecting')}
            </span>
          )}
          {v && (
            <span className="inline-flex items-center gap-1">
              {v.muted ? <MicOff className="size-3.5 text-alarm" aria-hidden="true" /> : <Mic className="size-3.5" aria-hidden="true" />}
              {v.muted ? t('In call, muted') : t('In call')}
            </span>
          )}
          {v?.camera && (
            <span className="inline-flex items-center gap-1">
              <Camera className="size-3.5" aria-hidden="true" /> {t('Camera on')}
            </span>
          )}
        </p>
      </div>
      {!isSelf && <VolumeControl peerId={person.id} name={person.display_name} />}
      {viewerIsHost && !isSelf && (
        <div className="flex items-center gap-1 opacity-100 sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100">
          {person.connected && !confirmKick && (
            <Button tone="dark" variant="ghost" size="sm" onClick={() => ctrl.hostTransfer(person.id)}>
              {t('Make host')}
            </Button>
          )}
          {confirmKick ? (
            <>
              <Button tone="dark" variant="danger" size="sm" onClick={() => ctrl.kick(person.id)}>
                {t('Remove {name}', { name: person.display_name })}
              </Button>
              <Button tone="dark" variant="ghost" size="sm" onClick={() => setConfirmKick(false)}>
                {t('Cancel')}
              </Button>
            </>
          ) : (
            <IconButton size="sm" label={t('Remove {name} from the room', { name: person.display_name })} onClick={() => setConfirmKick(true)}>
              <UserX className="size-4" />
            </IconButton>
          )}
        </div>
      )}
    </li>
  )
}
