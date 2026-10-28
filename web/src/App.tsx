import { lazy, Suspense } from 'react'
import { createBrowserRouter, RouterProvider } from 'react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ToastProvider } from './components/Toast'
import { I18nProvider } from './i18n'
import { Landing } from './pages/Landing'
import { NotFound } from './pages/NotFound'

// The room (player, WebRTC) and developer docs are split out of the landing bundle.
const RoomPage = lazy(() => import('./pages/RoomPage').then((m) => ({ default: m.RoomPage })))
const Developers = lazy(() => import('./pages/Developers').then((m) => ({ default: m.Developers })))

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 10_000, refetchOnWindowFocus: false } },
})

function Loading({ dark = false }: { dark?: boolean }) {
  return <div className={dark ? 'min-h-dvh bg-night' : 'min-h-dvh bg-paper'} aria-busy="true" />
}

const router = createBrowserRouter([
  { path: '/', element: <Landing /> },
  {
    path: '/r/:slug',
    element: (
      <Suspense fallback={<Loading dark />}>
        <RoomPage />
      </Suspense>
    ),
  },
  {
    path: '/developers',
    element: (
      <Suspense fallback={<Loading />}>
        <Developers />
      </Suspense>
    ),
  },
  { path: '*', element: <NotFound /> },
])

export function App() {
  return (
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <RouterProvider router={router} />
        </ToastProvider>
      </QueryClientProvider>
    </I18nProvider>
  )
}
