import { useEffect, useMemo, useRef, useState } from 'react'
import { Check, Copy, Link2, Share2, X } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import { useToast } from '../../components/Toast'
import { useI18n } from '../../i18n'
import { inviteMessage } from '../../lib/invite'
import { storage } from '../../lib/storage'
import { useController, useRoom, useSelf } from '../../room/context'

type Copied = 'message' | 'link' | null

/** "Invite friends": a ready-to-paste message with link, PIN and what's playing. */
export function InviteDialog({ onClose }: { onClose: () => void }) {
  const { t, lang } = useI18n()
  const ctrl = useController()
  const toast = useToast()
  const room = useRoom((s) => s.room)
  const nowPlaying = useRoom((s) => (s.playback?.status === 'playing' || s.playback?.status === 'paused' ? s.playback.item?.title : null))
  const self = useSelf()
  const ref = useRef<HTMLDialogElement>(null)
  const textRef = useRef<HTMLTextAreaElement>(null)
  const [pin, setPin] = useState(() => storage.getRoomPin(ctrl.slug) ?? '')
  const [copied, setCopied] = useState<Copied>(null)

  const link = `${window.location.origin}/r/${ctrl.slug}`
  const hasPin = !!room?.has_password
  const pinKnown = /^\d{4}$/.test(pin)

  const message = useMemo(
    () =>
      inviteMessage({
        inviter: self?.display_name ?? t('A friend'),
        roomName: room?.name ?? 'Partello',
        link,
        hasPin,
        pin: pinKnown ? pin : null,
        // Video titles carry " | Official Trailer | Channel"; keep the film name.
        nowPlaying: nowPlaying ? nowPlaying.split('|')[0]!.trim() : null,
        lang,
      }),
    [self?.display_name, room?.name, link, hasPin, pinKnown, pin, nowPlaying, lang, t],
  )

  useEffect(() => {
    const d = ref.current
    d?.showModal()
    return () => d?.close()
  }, [])

  const copy = async (what: Exclude<Copied, null>) => {
    const text = what === 'message' ? message : link
    try {
      await navigator.clipboard.writeText(text)
      setCopied(what)
      toast(what === 'message' ? t('Invitation copied. Paste it in any chat.') : t('Invite link copied. Send it to your friends.'))
      setTimeout(() => setCopied(null), 2000)
    } catch {
      // Clipboard blocked: select the text so the user can copy it by hand.
      textRef.current?.focus()
      textRef.current?.select()
      toast(t('Press Ctrl+C (or ⌘C) to copy the selected text.'))
    }
  }

  const share = async () => {
    try {
      await navigator.share({ title: room?.name ?? 'Partello', text: message })
    } catch {
      /* dismissed */
    }
  }

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      className="m-auto w-[min(94vw,32rem)] bg-night-2 p-0 text-fog shadow-[0_24px_60px_rgba(0,0,0,0.6)] backdrop:bg-black/70" /* taste-skill-ignore: a real modal dialog */
      aria-labelledby="invite-title"
    >
      <div className="flex flex-col gap-5 p-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="invite-title" className="text-lead font-semibold">
              {t('Invite friends')}
            </h2>
            <p className="pt-1 text-small text-haze">{t('Copy the invitation and paste it in WhatsApp, Discord or any chat.')}</p>
          </div>
          <IconButton size="sm" label={t('Close')} onClick={onClose}>
            <X className="size-4" />
          </IconButton>
        </div>

        {hasPin && (
          <label className="flex flex-wrap items-center gap-3">
            <span className="font-medium">{t('Room PIN')}</span>
            <input
              type="text"
              inputMode="numeric"
              pattern="[0-9]{4}"
              maxLength={4}
              value={pin}
              onChange={(e) => {
                const next = e.target.value.replace(/\D/g, '').slice(0, 4)
                setPin(next)
                if (next.length === 4) storage.setRoomPin(ctrl.slug, next)
              }}
              placeholder="••••"
              aria-describedby="invite-pin-hint"
              className="tabular h-10 w-28 bg-night-3 px-3 text-center text-lg tracking-[0.4em] text-fog placeholder:text-haze focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
            />
            <span id="invite-pin-hint" className="basis-full text-small text-haze">
              {pinKnown ? t('The PIN is included in the invitation.') : t('Type the PIN to include it, or friends will ask you for it.')}
            </span>
          </label>
        )}

        <label className="flex flex-col gap-2">
          <span className="text-small text-haze">{t('Invitation')}</span>
          <textarea
            ref={textRef}
            readOnly
            value={message}
            rows={message.split('\n').length + 1}
            onFocus={(e) => e.currentTarget.select()}
            className="scrollbar-thin resize-none bg-night-3 p-3 text-[0.9375rem] leading-relaxed text-fog focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright"
          />
        </label>

        <div className="flex flex-wrap gap-2">
          <Button
            tone="dark"
            variant={copied === 'message' ? 'ink' : 'primary'}
            onClick={() => void copy('message')}
            icon={copied === 'message' ? <Check className="size-4" /> : <Copy className="size-4" />}
            autoFocus
          >
            <span aria-live="polite">{copied === 'message' ? t('Copied') : t('Copy invitation')}</span>
          </Button>
          <Button
            tone="dark"
            variant="secondary"
            onClick={() => void copy('link')}
            icon={copied === 'link' ? <Check className="size-4" /> : <Link2 className="size-4" />}
          >
            {copied === 'link' ? t('Copied') : t('Copy link only')}
          </Button>
          {typeof navigator.share === 'function' && (
            <Button tone="dark" variant="secondary" onClick={() => void share()} icon={<Share2 className="size-4" />}>
              {t('Share')}
            </Button>
          )}
        </div>
      </div>
    </dialog>
  )
}
