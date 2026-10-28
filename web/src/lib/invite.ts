// Ready-to-paste invitation text, like the one video-call apps put on the
// clipboard. Pure so it can be unit tested; wording follows the UI language.

import { translate, type Lang } from '../i18n'

export interface InviteInput {
  inviter: string
  roomName: string
  link: string
  /** Shown only when the room has a PIN and it is known on this device. */
  pin?: string | null
  hasPin: boolean
  nowPlaying?: string | null
  lang?: Lang
}

export function inviteMessage(i: InviteInput): string {
  const tr = (text: string, vars?: Record<string, string>) => translate(text, vars, i.lang)
  const lines = [
    tr('{name} is inviting you to watch together on Partello 🍿', { name: i.inviter }),
    '',
    tr('Room: {room}', { room: i.roomName }),
    tr('Join: {link}', { link: i.link }),
  ]
  if (i.hasPin) lines.push(i.pin ? tr('PIN: {pin}', { pin: i.pin }) : tr('PIN: ask {name}', { name: i.inviter }))
  if (i.nowPlaying) lines.push('', tr('Now playing: {title}', { title: i.nowPlaying }))
  lines.push('', tr('No account or app needed. Open the link, type your name, and you are in.'))
  return lines.join('\n')
}
