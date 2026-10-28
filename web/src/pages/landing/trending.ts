// What the landing page shows: today's YouTube chart from the server
// (refreshed there once a day), or the built-in picks when the server has
// none. The featured stills are a random draw from the chart, made once per
// page load so every section shows the same draw.

import { useQuery } from '@tanstack/react-query'
import { api, type TrendingResponse } from '../../lib/api'
import { FEATURED, GENRES, type Genre, type GenreVideo } from './genres'

/** Genre names the server sends (server/src/trending.rs), for translation. */
export const TRENDING_GENRES = [
  'Trending',
  'Music',
  'Gaming',
  'Film & Animation',
  'Entertainment',
  'Comedy',
  'Sports',
  'Science & Technology',
] as const

/** Stills the page shows: hero (3), up next, and one per feature section. */
export const FEATURED_COUNT = 5

const BUILT_IN: Picks = {
  live: false,
  genres: GENRES,
  featured: [FEATURED.spongebob, FEATURED.spongebobTeaser, FEATURED.wildwood, FEATURED.notAlone, FEATURED.forgottenIsland],
}

export interface Picks {
  /** True when the picks are today's chart rather than the built-in list. */
  live: boolean
  genres: Genre[]
  /** Exactly FEATURED_COUNT distinct videos. */
  featured: GenreVideo[]
}

function shuffle<T>(list: T[]): T[] {
  const a = [...list]
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[a[i], a[j]] = [a[j]!, a[i]!]
  }
  return a
}

export function toPicks(r: TrendingResponse): Picks {
  const genres = r.genres.filter((g) => g.videos.length > 0)
  const seen = new Set<string>()
  const pool: GenreVideo[] = []
  for (const g of genres)
    for (const v of g.videos)
      if (!seen.has(v.id)) {
        seen.add(v.id)
        pool.push(v)
      }
  if (pool.length < FEATURED_COUNT) return BUILT_IN
  return { live: true, genres, featured: shuffle(pool).slice(0, FEATURED_COUNT) }
}

/** `undefined` while loading, so nothing flashes before the real pictures. */
export function usePicks(): Picks | undefined {
  const q = useQuery({
    queryKey: ['trending'],
    queryFn: async () => toPicks(await api.trending()),
    staleTime: Infinity,
    retry: 1,
  })
  if (q.isError) return BUILT_IN
  return q.data
}
