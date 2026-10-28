import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from 'react'
import { X } from 'lucide-react'
import { cx } from '../lib/format'
import { translate } from '../i18n'

type Tone = 'info' | 'error'
interface Toast {
  id: number
  message: string
  tone: Tone
}

const ToastContext = createContext<(message: string, tone?: Tone) => void>(() => {})

export function useToast() {
  return useContext(ToastContext)
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([])
  const seq = useRef(0)

  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), [])

  const push = useCallback(
    (message: string, tone: Tone = 'info') => {
      seq.current += 1
      const id = seq.current
      // Collapse identical messages fired in quick succession.
      setToasts((t) => (t.some((x) => x.message === message) ? t : [...t.slice(-3), { id, message, tone }]))
      setTimeout(() => dismiss(id), tone === 'error' ? 6000 : 3500)
    },
    [dismiss],
  )

  const value = useMemo(() => push, [push])

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex flex-col items-center gap-2 px-4 sm:bottom-6"
        aria-live="polite"
        role="status"
      >
        {toasts.map((t) => (
          <div
            key={t.id}
            className={cx(
              'pop-in pointer-events-auto flex max-w-md items-start gap-3 px-4 py-3 text-[0.9375rem] shadow-[0_8px_30px_rgba(0,0,0,0.35)]',
              t.tone === 'error' ? 'bg-[#3a1212] text-[#ffdcdc]' : 'bg-fog text-night',
            )}
          >
            <span className="flex-1">{t.message}</span>
            <button className="-m-1 p-1 opacity-70 hover:opacity-100" aria-label={translate('Dismiss')} onClick={() => dismiss(t.id)}>
              <X className="size-4" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  )
}
