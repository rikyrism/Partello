import { useState } from 'react'
import { BellPlus, BellRing } from 'lucide-react'
import { IconButton } from '../../components/Button'
import { useToast } from '../../components/Toast'
import { useController } from '../../room/context'
import { PushDenied, isSubscribed, pushSupport, subscribeRoom, unsubscribeRoom } from '../../lib/pwa'
import { ApiError } from '../../lib/api'
import { useI18n } from '../../i18n'

/** "Tell me when someone starts watching here", even with Partello closed. */
export function NotifyButton() {
  const { t, lang } = useI18n()
  const ctrl = useController()
  const toast = useToast()
  const [on, setOn] = useState(() => isSubscribed(ctrl.slug))
  const [busy, setBusy] = useState(false)
  const support = pushSupport()
  if (support === 'no') return null

  const toggle = async () => {
    if (support === 'install-first') {
      toast(t('On iPhone and iPad, add Partello to your Home Screen first (Share → Add to Home Screen), then turn on notifications there.'))
      return
    }
    setBusy(true)
    try {
      if (on) {
        await unsubscribeRoom(ctrl.slug, ctrl.token)
        setOn(false)
        toast(t('Notifications for this room are off'))
      } else {
        await subscribeRoom(ctrl.slug, ctrl.token, lang)
        setOn(true)
        toast(t("We'll let you know when someone starts watching here"))
      }
    } catch (err) {
      if (err instanceof PushDenied) toast(t('Notifications are blocked. Allow them in your browser settings for this site.'), 'error')
      else if (err instanceof ApiError) toast(err.message, 'error')
      else toast(t("Couldn't turn on notifications in this browser."), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <IconButton
      label={on ? t('Stop notifications for this room') : t('Notify me when someone starts watching here')}
      onClick={() => void toggle()}
      disabled={busy}
      active={on}
      aria-pressed={on}
    >
      {on ? <BellRing className="size-[18px]" /> : <BellPlus className="size-[18px]" />}
    </IconButton>
  )
}
