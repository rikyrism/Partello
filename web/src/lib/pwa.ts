// Installable app and push notifications. Everything here degrades quietly:
// browsers without service workers or push simply don't offer the features.

import { api } from './api'

const pushKey = (slug: string) => `partello:push:${slug}`

export function registerServiceWorker(): void {
  if (!('serviceWorker' in navigator) || !import.meta.env.PROD) return
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch((err) => console.warn('[pwa] service worker failed', err))
  })
}

/** Running as an installed app (home screen / dock). */
export function isStandalone(): boolean {
  return window.matchMedia?.('(display-mode: standalone)').matches || (navigator as { standalone?: boolean }).standalone === true
}

export type PushSupport = 'yes' | 'install-first' | 'no'

/** iPhone and iPad only allow push for apps added to the Home Screen. */
export function pushSupport(): PushSupport {
  const hasSw = 'serviceWorker' in navigator
  if (hasSw && 'PushManager' in window && 'Notification' in window) return 'yes'
  const ios = /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)
  return ios && hasSw && !isStandalone() ? 'install-first' : 'no'
}

export function isSubscribed(slug: string): boolean {
  try {
    return !!localStorage.getItem(pushKey(slug))
  } catch {
    return false
  }
}

function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
  const b64 = base64url.replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(b64 + '='.repeat((4 - (b64.length % 4)) % 4))
  return Uint8Array.from(raw, (c) => c.charCodeAt(0))
}

export class PushDenied extends Error {}

/** Asks for permission (must run from a click) and registers this browser for the room. */
export async function subscribeRoom(slug: string, token: string, lang: string): Promise<void> {
  const permission = await Notification.requestPermission()
  if (permission !== 'granted') throw new PushDenied('denied')
  const [{ public_key }, reg] = await Promise.all([api.pushKey(), navigator.serviceWorker.ready])
  const sub =
    (await reg.pushManager.getSubscription()) ??
    (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: keyBytes(public_key) }))
  await api.subscribePush(slug, token, sub.toJSON(), lang)
  try {
    localStorage.setItem(pushKey(slug), sub.endpoint)
  } catch {
    /* the server still has it */
  }
}

/** Stops notifications for one room; the browser stays subscribed for other rooms. */
export async function unsubscribeRoom(slug: string, token: string): Promise<void> {
  let endpoint: string | null = null
  try {
    endpoint = localStorage.getItem(pushKey(slug))
    localStorage.removeItem(pushKey(slug))
  } catch {
    /* fall back to the live subscription */
  }
  if (!endpoint && 'serviceWorker' in navigator) {
    endpoint = (await (await navigator.serviceWorker.ready).pushManager.getSubscription())?.endpoint ?? null
  }
  if (endpoint) await api.unsubscribePush(slug, token, endpoint)
}

/** A system notification while the tab is in the background (mentions). */
export async function notifyLocally(title: string, body: string, tag: string): Promise<void> {
  if (!('Notification' in window) || Notification.permission !== 'granted') return
  const options = { body, tag, icon: '/icons/icon-192.png', badge: '/icons/badge-96.png' }
  try {
    const reg = 'serviceWorker' in navigator ? await navigator.serviceWorker.getRegistration() : undefined
    if (reg) await reg.showNotification(title, options)
    else new Notification(title, options)
  } catch {
    /* some browsers refuse page notifications; the sound still plays */
  }
}
