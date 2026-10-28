import { useEffect, useState } from 'react'
import { Download } from 'lucide-react'
import { useT } from '../i18n'

interface InstallPromptEvent extends Event {
  prompt(): Promise<void>
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>
}

/** "Install app", shown only when the browser offers installing Partello. */
export function InstallButton() {
  const t = useT()
  const [prompt, setPrompt] = useState<InstallPromptEvent | null>(null)

  useEffect(() => {
    const offer = (e: Event) => {
      e.preventDefault()
      setPrompt(e as InstallPromptEvent)
    }
    const installed = () => setPrompt(null)
    window.addEventListener('beforeinstallprompt', offer)
    window.addEventListener('appinstalled', installed)
    return () => {
      window.removeEventListener('beforeinstallprompt', offer)
      window.removeEventListener('appinstalled', installed)
    }
  }, [])

  if (!prompt) return null
  return (
    <button
      type="button"
      onClick={async () => {
        await prompt.prompt()
        await prompt.userChoice
        setPrompt(null)
      }}
      className="hidden h-9 items-center gap-1.5 px-2 hover:underline sm:inline-flex"
    >
      <Download className="size-4" aria-hidden="true" />
      {t('Install app')}
    </button>
  )
}
