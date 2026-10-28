// Partello service worker: makes the app installable, keeps the app shell
// available offline, and shows "someone started watching" push notifications.
// API calls and WebSockets are never cached or intercepted.

const CACHE = 'partello-v1'

self.addEventListener('install', () => self.skipWaiting())

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      for (const key of await caches.keys()) if (key !== CACHE) await caches.delete(key)
      await self.clients.claim()
    })(),
  )
})

self.addEventListener('fetch', (event) => {
  const req = event.request
  const url = new URL(req.url)
  if (req.method !== 'GET' || url.origin !== self.location.origin) return
  if (url.pathname.startsWith('/api/') || url.pathname === '/healthz' || url.pathname === '/readyz') return

  // Pages: always the newest from the network; the last copy when offline.
  if (req.mode === 'navigate') {
    event.respondWith(
      (async () => {
        try {
          const fresh = await fetch(req)
          const cache = await caches.open(CACHE)
          await cache.put('/', fresh.clone())
          return fresh
        } catch {
          return (await caches.match('/')) ?? Response.error()
        }
      })(),
    )
    return
  }

  // Hashed build files and icons never change: cache first.
  if (url.pathname.startsWith('/assets/') || url.pathname.startsWith('/icons/')) {
    event.respondWith(
      (async () => {
        const hit = await caches.match(req)
        if (hit) return hit
        const res = await fetch(req)
        if (res.ok) (await caches.open(CACHE)).put(req, res.clone())
        return res
      })(),
    )
  }
})

self.addEventListener('push', (event) => {
  let notice = {}
  try {
    notice = event.data ? event.data.json() : {}
  } catch {
    return
  }
  const url = typeof notice.url === 'string' && notice.url.startsWith('/') ? notice.url : '/'
  event.waitUntil(
    (async () => {
      // Already watching in this room: no need to be told.
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true })
      if (windows.some((w) => w.visibilityState === 'visible' && new URL(w.url).pathname === url)) return
      await self.registration.showNotification(notice.title || 'Partello', {
        body: notice.body || '',
        tag: notice.tag,
        renotify: true,
        icon: '/icons/icon-192.png',
        badge: '/icons/badge-96.png',
        data: { url },
      })
    })(),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const url = (event.notification.data && event.notification.data.url) || '/'
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true })
      const open = windows.find((w) => new URL(w.url).pathname === url)
      if (open) return open.focus()
      return self.clients.openWindow(url)
    })(),
  )
})
