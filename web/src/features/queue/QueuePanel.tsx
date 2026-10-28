import { useEffect, useState, type FormEvent, type KeyboardEvent } from 'react'
import { ArrowUp, Check, ListPlus, Play, Plus, Search, X } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import { useToast } from '../../components/Toast'
import { useCanControl, useController, useRoom, useSelf } from '../../room/context'
import type { QueueItem } from '../../realtime/protocol'
import { parseYouTubeId, playlistIdOf } from '../../lib/youtube'
import { cx, formatTime } from '../../lib/format'
import { api, ApiError, type SearchResult } from '../../lib/api'
import { translate, useT } from '../../i18n'

export function QueuePanel() {
  const t = useT()
  const queue = useRoom((s) => s.queue)
  const playback = useRoom((s) => s.playback)
  const synced = useRoom((s) => s.synced)
  const now = playback?.item && playback.status !== 'idle' ? playback.item : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <AddVideoForm />
      <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-3 pb-4">
        {!synced ? (
          <div className="mt-3 h-20 animate-pulse bg-night-3" aria-label={t('Loading queue')} />
        ) : (
          <>
            {now && (
              <section aria-label={t('Now playing')} className="pt-3">
                <h3 className="pb-2 text-small text-haze">{playback?.status === 'ended' ? t('Last played') : t('Now playing')}</h3>
                <VideoRow item={now} current />
              </section>
            )}
            <section aria-label={t('Up next')} className="pt-4">
              <h3 className="pb-2 text-small text-haze">{t('Up next')}{queue.length > 0 ? ` · ${queue.length}` : ''}</h3>
              {queue.length === 0 ? (
                <p className="text-haze">{t('The queue is empty. Add a link above and it plays after the current video.')}</p>
              ) : (
                <ol className="flex flex-col gap-2">
                  {queue.map((item, i) => (
                    <li key={item.id}>
                      <VideoRow item={item} index={i} />
                    </li>
                  ))}
                </ol>
              )}
            </section>
          </>
        )}
      </div>
    </div>
  )
}

function AddVideoForm() {
  const t = useT()
  const ctrl = useController()
  const self = useSelf()
  const queueMode = useRoom((s) => s.room?.settings.queue_mode)
  const connected = useRoom((s) => s.status === 'open')
  const searchEnabled = useRoom((s) => !!s.limits?.search_enabled)
  const playlistImport = useRoom((s) => !!s.limits?.playlist_import)
  const toast = useToast()
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(-1)
  const [added, setAdded] = useState<Set<string>>(() => new Set())
  const allowed = queueMode !== 'host_only' || !!self?.is_host

  const isLink = looksLikeLink(value)
  const playlistId = isLink ? playlistIdOf(value) : null
  const query = !isLink && searchEnabled ? value.trim() : ''
  const search = useYouTubeSearch(ctrl.slug, ctrl.token, query)
  const results = search.results
  const showList = open && query.length >= 2

  const add = async (url: string, videoId?: string) => {
    setBusy(true)
    setError(null)
    try {
      await ctrl.queueAdd(url)
      if (videoId) setAdded((a) => new Set(a).add(videoId))
      else setValue('')
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const addPlaylist = async () => {
    setBusy(true)
    setError(null)
    try {
      await ctrl.queueAdd(value.trim(), true)
      setValue('')
      toast(t('Playlist added to the queue'))
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!value.trim()) return
    if (showList && active >= 0 && results[active]) {
      const r = results[active]
      void add(`https://www.youtube.com/watch?v=${r.video_id}`, r.video_id)
      return
    }
    if (!parseYouTubeId(value)) {
      // A playlist page link has no single video in it.
      if (playlistId) {
        if (playlistImport) void addPlaylist()
        else setError(t("Playlist import isn't set up on this server. Add the videos one by one."))
        return
      }
      setError(searchEnabled ? t('Pick a video from the results, or paste a YouTube link.') : t("That doesn't look like a YouTube video link."))
      return
    }
    void add(value.trim())
  }

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (!showList || results.length === 0) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActive((i) => (i + 1) % results.length)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActive((i) => (i <= 0 ? results.length - 1 : i - 1))
    } else if (e.key === 'Escape') {
      setOpen(false)
    }
  }

  if (!allowed) {
    return <p className="bg-night-2 px-3 py-3 text-small text-haze">{t('Only the host can add videos in this room.')}</p>
  }

  const activeId = showList && active >= 0 && results[active] ? `search-opt-${results[active].video_id}` : undefined

  return (
    <div className="relative z-20 bg-night-2 p-2">
      <form onSubmit={submit} noValidate className="flex gap-2">
        <label htmlFor="queue-url" className="sr-only">
          {searchEnabled ? t('Search YouTube or paste a link') : t('YouTube link')}
        </label>
        <div className="relative min-w-0 flex-1">
          {searchEnabled && !isLink && <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-haze" aria-hidden="true" />}
          <input
            id="queue-url"
            type="text"
            autoComplete="off"
            spellCheck={false}
            role={searchEnabled ? 'combobox' : undefined}
            aria-expanded={searchEnabled ? showList : undefined}
            aria-controls={searchEnabled ? 'search-results' : undefined}
            aria-activedescendant={activeId}
            aria-autocomplete={searchEnabled ? 'list' : undefined}
            value={value}
            onChange={(e) => {
              setValue(e.target.value)
              setError(null)
              setOpen(true)
              setActive(-1)
            }}
            onFocus={() => setOpen(true)}
            onKeyDown={onKeyDown}
            placeholder={searchEnabled ? t('Search YouTube or paste a link') : t('Paste a YouTube link')}
            aria-invalid={!!error}
            aria-describedby={error ? 'queue-error' : undefined}
            className={cx(
              'h-11 w-full bg-night-3 pr-3 text-[0.9375rem] text-fog placeholder:text-haze focus:outline-none focus-visible:outline-2 focus-visible:outline-biro-bright',
              searchEnabled && !isLink ? 'pl-9' : 'pl-3',
            )}
          />
        </div>
        <Button type="submit" tone="dark" loading={busy && !showList} disabled={!connected || !value.trim()} icon={<Plus className="size-4" />}>
          {t('Add')}
        </Button>
      </form>
      {playlistId && playlistImport && parseYouTubeId(value) && (
        <div className="flex flex-wrap items-center gap-2 px-1 pt-2 text-small text-haze">
          <span className="flex-1">{t('This link is part of a playlist.')}</span>
          <Button size="sm" tone="dark" variant="secondary" disabled={!connected || busy} onClick={() => void addPlaylist()} icon={<ListPlus className="size-4" />}>
            {t('Add the whole playlist')}
          </Button>
        </div>
      )}
      {error && (
        <p id="queue-error" role="alert" className="px-1 pt-2 text-small text-alarm">
          {error}
        </p>
      )}
      {showList && (
        <div className="pop-in absolute inset-x-2 top-full z-30 max-h-[min(60vh,28rem)] overflow-y-auto bg-night-3 shadow-[0_18px_40px_rgba(0,0,0,0.55)] scrollbar-thin">
          {search.status === 'loading' && results.length === 0 && <SearchSkeleton />}
          {search.status === 'error' && <p className="p-3 text-small text-alarm">{search.error}</p>}
          {search.status === 'done' && results.length === 0 && <p className="p-3 text-small text-haze">{t('No playable videos found for "{query}".', { query })}</p>}
          {results.length > 0 && (
            <ul id="search-results" role="listbox" aria-label={t('YouTube results')} className={cx(search.status === 'loading' && 'opacity-60')}>
              {results.map((r, i) => {
                const done = added.has(r.video_id)
                return (
                  <li
                    key={r.video_id}
                    id={`search-opt-${r.video_id}`}
                    role="option"
                    aria-selected={i === active}
                    onMouseEnter={() => setActive(i)}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => !done && void add(`https://www.youtube.com/watch?v=${r.video_id}`, r.video_id)}
                    className={cx('flex cursor-pointer gap-3 p-2', i === active ? 'bg-night-4' : '')}
                  >
                    <span className="relative aspect-video w-28 shrink-0 bg-night-2">
                      <img src={r.thumbnail_url} alt="" loading="lazy" className="size-full object-cover" />
                      <span className="tabular absolute right-1 bottom-1 bg-black/80 px-1 text-xs text-white">
                        {r.is_live ? t('Live') : r.duration ? formatTime(r.duration) : ''}
                      </span>
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col justify-center">
                      <span className="line-clamp-2 text-[0.9375rem] leading-snug font-medium text-fog">{r.title}</span>
                      <span className="truncate pt-0.5 text-small text-haze">{r.channel}</span>
                    </span>
                    <span className="grid w-8 shrink-0 place-items-center text-haze" aria-hidden="true">
                      {done ? <Check className="size-4 text-biro-bright" /> : <Plus className="size-4" />}
                    </span>
                  </li>
                )
              })}
            </ul>
          )}
          <p className="px-3 py-2 text-xs text-haze">{t('Results from YouTube. Only videos that can play here are shown.')}</p>
        </div>
      )}
    </div>
  )
}

function SearchSkeleton() {
  const t = useT()
  return (
    <div className="flex flex-col gap-2 p-2" aria-label={t('Searching')}>
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex gap-3">
          <div className="aspect-video w-28 animate-pulse bg-night-4" />
          <div className="flex flex-1 flex-col gap-2 pt-2">
            <div className="h-3 w-4/5 animate-pulse bg-night-4" />
            <div className="h-3 w-1/3 animate-pulse bg-night-4" />
          </div>
        </div>
      ))}
    </div>
  )
}

/** Anything that looks like a URL is treated as a link, not a search. */
function looksLikeLink(v: string): boolean {
  const t = v.trim()
  return /^https?:\/\//i.test(t) || /^(www\.|m\.)?(youtube\.com|youtu\.be|music\.youtube\.com)\//i.test(t)
}

type SearchState = { status: 'idle' | 'loading' | 'done' | 'error'; results: SearchResult[]; error?: string }

/** Debounced, cancellable search; keeps previous results visible while typing. */
function useYouTubeSearch(slug: string, token: string, query: string): SearchState {
  const [state, setState] = useState<SearchState>({ status: 'idle', results: [] })
  useEffect(() => {
    if (query.length < 2) return
    const controller = new AbortController()
    const timer = setTimeout(() => {
      setState((s) => ({ ...s, status: 'loading' }))
      api
        .search(slug, token, query, controller.signal)
        .then((r) => setState({ status: 'done', results: r.results }))
        .catch((err) => {
          if (controller.signal.aborted) return
          setState({ status: 'error', results: [], error: err instanceof ApiError ? err.message : translate('Search failed') })
        })
    }, 350)
    return () => {
      clearTimeout(timer)
      controller.abort()
    }
  }, [slug, token, query])
  return query.length < 2 ? { status: 'idle', results: [] } : state
}

function VideoRow({ item, index, current = false }: { item: QueueItem; index?: number; current?: boolean }) {
  const t = useT()
  const ctrl = useController()
  const canControl = useCanControl()
  const self = useSelf()
  const queueMode = useRoom((s) => s.room?.settings.queue_mode)
  const canRemove = !current && (!!self?.is_host || (queueMode === 'everyone' && item.added_by === self?.id))

  return (
    <div className={cx('group flex gap-3 p-2', current ? 'bg-night-3' : 'bg-night-2 hover:bg-night-3')}>
      <img src={item.thumbnail_url} alt="" loading="lazy" className="aspect-video w-28 shrink-0 bg-night-4 object-cover" />
      <div className="flex min-w-0 flex-1 flex-col justify-center">
        <p className="line-clamp-2 text-[0.9375rem] font-medium leading-snug text-fog" title={item.title}>
          {item.title}
        </p>
        <p className="truncate pt-0.5 text-small text-haze">
          {item.author ? `${item.author} · ` : ''}{t('added by {name}', { name: item.added_by_name })}
        </p>
      </div>
      {!current && (
        <div className="flex shrink-0 items-center gap-1 opacity-100 sm:opacity-0 sm:transition-opacity sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
          {canControl && (
            <IconButton size="sm" label={t('Play "{title}" now', { title: item.title })} onClick={() => ctrl.queuePlay(item.id)}>
              <Play className="size-4" />
            </IconButton>
          )}
          {canControl && index !== undefined && index > 0 && (
            <IconButton size="sm" label={t('Move "{title}" up', { title: item.title })} onClick={() => ctrl.queueMove(item.id, index - 1)}>
              <ArrowUp className="size-4" />
            </IconButton>
          )}
          {canRemove && (
            <IconButton size="sm" label={t('Remove "{title}"', { title: item.title })} onClick={() => ctrl.queueRemove(item.id)}>
              <X className="size-4" />
            </IconButton>
          )}
        </div>
      )}
    </div>
  )
}
