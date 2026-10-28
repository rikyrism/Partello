// Tiny i18n: English is the source language and the default. Strings are
// written in English at the call site, `t('Join call')`, and looked up in the
// Indonesian dictionary when the visitor switches language. A unit test scans
// the source for `t('…')` literals and fails if a translation is missing.
//
// Interpolation: `t('{n} people are here', { n: 3 })`.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { id } from './id'

export type Lang = 'en' | 'id'
const STORAGE_KEY = 'partello:lang'

let currentLang: Lang = readStoredLang()

function readStoredLang(): Lang {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'id' ? 'id' : 'en'
  } catch {
    return 'en'
  }
}

export type Vars = Record<string, string | number>

/** Marks a string for translation where it is defined (lists of content); render it with t(). */
export const msg = (text: string) => text

function interpolate(text: string, vars?: Vars): string {
  if (!vars) return text
  return text.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m))
}

/** Translate outside React (toasts from non-component code, server messages). */
export function translate(text: string, vars?: Vars, lang: Lang = currentLang): string {
  if (lang !== 'id') return interpolate(text, vars)
  if (text in id) return interpolate(id[text]!, vars)
  // Server messages carry numbers from configuration ("The queue is full (50
  // videos)"): match them against templates written with {n}.
  const numbers = text.match(/\d+/g)
  if (numbers) {
    const template = id[text.replace(/\d+/g, '{n}')]
    if (template) {
      let i = 0
      return template.replace(/\{n\}/g, () => numbers[i++] ?? '')
    }
  }
  return interpolate(text, vars)
}

/** Picks the right variant of content that is not a single string (FAQ lists, etc.). */
export function pick<T>(byLang: Record<Lang, T>, lang: Lang = currentLang): T {
  return byLang[lang]
}

interface I18n {
  lang: Lang
  setLang: (l: Lang) => void
  t: (text: string, vars?: Vars) => string
}

const I18nContext = createContext<I18n>({ lang: 'en', setLang: () => {}, t: (s, v) => interpolate(s, v) })

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(currentLang)

  useEffect(() => {
    currentLang = lang
    document.documentElement.lang = lang
  }, [lang])

  // Pages that set their own title (the room) override this after mount.
  useEffect(() => {
    if (document.title.startsWith('Partello —')) document.title = translate('Partello — Watch together. Wherever.', undefined, lang)
  }, [lang])

  const setLang = useCallback((l: Lang) => {
    currentLang = l
    try {
      localStorage.setItem(STORAGE_KEY, l)
    } catch {
      /* private mode: the choice lasts for this page only */
    }
    setLangState(l)
  }, [])

  const value = useMemo<I18n>(() => ({ lang, setLang, t: (text, vars) => translate(text, vars, lang) }), [lang, setLang])
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n(): I18n {
  return useContext(I18nContext)
}

export function useT() {
  return useContext(I18nContext).t
}
