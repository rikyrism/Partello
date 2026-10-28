// The room's YouTube player. The server's playback state is the truth; this
// component makes the local IFrame player follow it, and turns the user's own
// clicks on the YouTube controls into room commands when they are allowed to.
//
// Loop prevention:
//  * programmatic changes open a short "suppress" window during which player
//    events are not treated as user intent;
//  * player events count as user intent only while the YouTube iframe has
//    focus (the user clicked into it);
//  * the server's echo of our own command is applied idempotently (no seek
//    when already within tolerance).

import { useCallback, useEffect, useRef, useState } from 'react'
import { Lock, Pause, Play, RefreshCw, Settings2, SkipForward, Volume2 } from 'lucide-react'
import type { Playback } from '../../realtime/protocol'
import { useCanControl, useController, useRoom } from '../../room/context'
import { useToast } from '../../components/Toast'
import { useCall } from '../call/CallProvider'
import { translate, useT } from '../../i18n'
import { Button, IconButton } from '../../components/Button'
import { cx, formatTime } from '../../lib/format'
import { DRIFT_TOLERANCE, decideCorrection, detectJump, expectedPosition, playerErrorMessage } from './syncMath'
import { YTState, loadYouTubeApi, type YTPlayer } from './youtubeApi'
import { ReactionOverlay } from '../reactions/Reactions'
import { SkipVoteButton } from './SkipVote'

const POLL_MS = 500
const playerLanguage = (navigator.language || 'en').split('-')[0] ?? 'en'
type Blocked = 'none' | 'muted' | 'needs_click'

export function SyncedPlayer() {
  const t = useT()
  const ctrl = useController()
  const toast = useToast()
  const playback = useRoom((s) => s.playback)
  const canControl = useCanControl()

  const hostRef = useRef<HTMLDivElement>(null)
  const playerRef = useRef<YTPlayer | null>(null)
  const [ready, setReady] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [videoError, setVideoError] = useState<string | null>(null)
  const [blocked, setBlocked] = useState<Blocked>('none')
  const [localDuration, setLocalDuration] = useState<number | null>(null)

  // Latest values for imperative callbacks.
  const playbackRef = useRef<Playback | null>(playback)
  const canControlRef = useRef(canControl)
  const loadedItemRef = useRef<string | null>(null)
  const suppressUntilRef = useRef(0)
  const lastCorrectionRef = useRef(0)
  const durationReportedRef = useRef<string | null>(null)
  const sampleRef = useRef<{ time: number; at: number; playing: boolean } | null>(null)
  const autoplayTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const blockedRef = useRef<Blocked>('none')

  useEffect(() => {
    playbackRef.current = playback
  }, [playback])
  useEffect(() => {
    canControlRef.current = canControl
  }, [canControl])
  useEffect(() => {
    blockedRef.current = blocked
  }, [blocked])

  const suppress = (ms: number) => {
    suppressUntilRef.current = Date.now() + ms
    sampleRef.current = null
  }

  const iframeFocused = () => {
    const p = playerRef.current
    return !!p && document.activeElement === p.getIframe()
  }

  const reportDuration = useCallback(() => {
    const p = playerRef.current
    const item = playbackRef.current?.item
    if (!p || !item) return
    const d = p.getDuration()
    if (d > 0) {
      setLocalDuration(d)
      if (durationReportedRef.current !== item.id) {
        durationReportedRef.current = item.id
        ctrl.reportDuration(item.id, d)
      }
    }
  }, [ctrl])

  /** If the browser refused to autoplay, fall back to muted, then to a click. */
  const checkAutoplay = useCallback(() => {
    clearTimeout(autoplayTimerRef.current)
    autoplayTimerRef.current = setTimeout(() => {
      const p = playerRef.current
      if (!p || playbackRef.current?.status !== 'playing') return
      const state = p.getPlayerState()
      if (state === YTState.PLAYING || state === YTState.BUFFERING) return
      if (p.isMuted()) {
        setBlocked('needs_click')
        return
      }
      p.mute()
      p.playVideo()
      setBlocked('muted')
      autoplayTimerRef.current = setTimeout(() => {
        const s = playerRef.current?.getPlayerState()
        if (playbackRef.current?.status === 'playing' && s !== YTState.PLAYING && s !== YTState.BUFFERING) setBlocked('needs_click')
      }, 2000)
    }, 2000)
  }, [])

  /** Make the local player match the room. */
  const apply = useCallback(() => {
    const p = playerRef.current
    const pb = playbackRef.current
    if (!p || !pb) return
    if (!pb.item || pb.status === 'idle') {
      if (loadedItemRef.current) {
        p.pauseVideo()
        loadedItemRef.current = null
      }
      return
    }
    const target = expectedPosition(pb, ctrl.clock.now())

    if (pb.item.id !== loadedItemRef.current) {
      loadedItemRef.current = pb.item.id
      setVideoError(null)
      setLocalDuration(null)
      suppress(2000)
      if (pb.status === 'playing') {
        p.loadVideoById({ videoId: pb.item.video_id, startSeconds: target })
        checkAutoplay()
      } else {
        p.cueVideoById({ videoId: pb.item.video_id, startSeconds: target })
      }
      return
    }

    const state = p.getPlayerState()
    const current = p.getCurrentTime()
    suppress(900)
    if (pb.status === 'playing') {
      if (Math.abs(current - target) > DRIFT_TOLERANCE) p.seekTo(target, true)
      if (state !== YTState.PLAYING && state !== YTState.BUFFERING) {
        p.playVideo()
        checkAutoplay()
      }
    } else {
      if (state === YTState.PLAYING || state === YTState.BUFFERING) p.pauseVideo()
      if (Math.abs(current - target) > 0.5) {
        // seekTo on a cued video would start it, so re-cue at the new spot instead.
        if (state === YTState.CUED || state === YTState.UNSTARTED) {
          p.cueVideoById({ videoId: pb.item.video_id, startSeconds: target })
        } else {
          p.seekTo(target, true)
        }
      }
    }
    lastCorrectionRef.current = Date.now()
  }, [ctrl, checkAutoplay])

  // Create the YouTube player once.
  useEffect(() => {
    let cancelled = false
    const host = hostRef.current
    if (!host) return
    const mount = document.createElement('div')
    host.appendChild(mount)

    loadYouTubeApi()
      .then((YT) => {
        if (cancelled) return
        playerRef.current = new YT.Player(mount, {
          width: '100%',
          height: '100%',
          playerVars: {
            controls: 1,
            rel: 0,
            playsinline: 1,
            enablejsapi: 1,
            origin: window.location.origin,
            // YouTube's own menus (quality, subtitles) and default caption
            // language follow each viewer's browser language.
            hl: playerLanguage,
            cc_lang_pref: playerLanguage,
          },
          events: {
            onReady: () => {
              if (!cancelled) setReady(true)
            },
            onAutoplayBlocked: () => {
              const p = playerRef.current
              if (p && !p.isMuted()) {
                p.mute()
                p.playVideo()
                setBlocked('muted')
              }
            },
            onError: (e) => setVideoError(translate(playerErrorMessage(e.data))),
            // Speed is not shared across the room, so a faster local player would
            // drift away from everyone else. Keep it at 1x and say why.
            onPlaybackRateChange: (e) => {
              if (e.data !== 1) {
                e.target.setPlaybackRate(1)
                toast(translate('Playback speed stays at 1× so everyone keeps watching together'))
              }
            },
            onStateChange: (e) => onStateChange(e.data),
          },
        })
      })
      .catch((err: Error) => setLoadError(err.message))

    function onStateChange(state: number) {
      const p = playerRef.current
      const pb = playbackRef.current
      if (!p || !pb?.item) return
      if (state === YTState.PLAYING || state === YTState.CUED) reportDuration()
      if (state === YTState.PLAYING && blockedRef.current === 'needs_click') setBlocked(p.isMuted() ? 'muted' : 'none')
      if (state === YTState.ENDED) {
        ctrl.reportEnded(pb.item.id)
        return
      }
      if (Date.now() < suppressUntilRef.current || !iframeFocused()) return

      // The user clicked play/pause on the YouTube controls.
      const wantsPlay = state === YTState.PLAYING && pb.status !== 'playing'
      const wantsPause = state === YTState.PAUSED && pb.status === 'playing'
      if (!wantsPlay && !wantsPause) return
      if (canControlRef.current) {
        ctrl.control(wantsPlay ? 'play' : 'pause', p.getCurrentTime())
        suppress(900)
      } else {
        toast(translate('Only the host can control playback in this room'))
        apply()
      }
    }

    return () => {
      cancelled = true
      clearTimeout(autoplayTimerRef.current)
      playerRef.current?.destroy()
      playerRef.current = null
      loadedItemRef.current = null
      host.replaceChildren()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // While in a call the video plays a little quieter, and much quieter while
  // anyone talks, so voices stay clear and less video leaks into microphones.
  const call = useCall()
  const inCall = call.status === 'joined' && call.autoDuck
  const baseVolumeRef = useRef<number | null>(null)
  useEffect(() => {
    const p = playerRef.current
    if (!p || !ready) return
    const factor = !inCall ? 1 : call.ducking ? 0.2 : 0.65
    if (factor === 1) {
      if (baseVolumeRef.current != null) p.setVolume(baseVolumeRef.current)
      baseVolumeRef.current = null
      return
    }
    baseVolumeRef.current ??= p.getVolume()
    p.setVolume(Math.round(baseVolumeRef.current * factor))
  }, [ready, inCall, call.ducking])

  // Follow every authoritative change.
  useEffect(() => {
    if (ready) apply()
  }, [ready, playback, apply])

  // Periodic drift correction and seek detection.
  useEffect(() => {
    if (!ready) return
    const id = setInterval(() => {
      const p = playerRef.current
      const pb = playbackRef.current
      if (!p || !pb?.item || pb.status === 'idle' || loadedItemRef.current !== pb.item.id) return
      const state = p.getPlayerState()
      const now = Date.now()
      const time = p.getCurrentTime()
      const prev = sampleRef.current
      sampleRef.current = { time, at: now, playing: state === YTState.PLAYING }
      if (now < suppressUntilRef.current) return
      if (state !== YTState.PLAYING && state !== YTState.PAUSED) return

      const userJumped = !!prev && iframeFocused() && detectJump(prev.time, prev.at, time, now, prev.playing)
      const decision = decideCorrection({
        playback: pb,
        serverNow: ctrl.clock.now(),
        localTime: time,
        userJumped,
        canControl: canControlRef.current,
        msSinceLastCorrection: now - lastCorrectionRef.current,
      })
      if (decision.kind === 'user_seek') {
        ctrl.control('seek', decision.to)
        suppress(900)
        lastCorrectionRef.current = now
      } else if (decision.kind === 'seek') {
        if (userJumped && !canControlRef.current) toast(translate('Only the host can control playback in this room'))
        p.seekTo(decision.to, true)
        suppress(900)
        lastCorrectionRef.current = now
      }
    }, POLL_MS)
    return () => clearInterval(id)
  }, [ready, ctrl, toast])

  const startManually = () => {
    const p = playerRef.current
    if (!p) return
    setBlocked(p.isMuted() ? 'muted' : 'none')
    apply()
    p.playVideo()
  }

  const unmute = () => {
    playerRef.current?.unMute()
    setBlocked('none')
  }

  const idle = !playback?.item || playback.status === 'idle'

  return (
    <section aria-label={t('Video player')} className="flex flex-col">
      <div className="relative aspect-video w-full overflow-hidden bg-black">
        <div ref={hostRef} className={cx('absolute inset-0 [&>iframe]:size-full', idle && 'invisible')} />
        {idle && <EmptyStage canAdd />}
        <ReactionOverlay />
        {loadError && (
          <div className="absolute inset-0 grid place-items-center bg-night-2 p-6 text-center text-fog">
            <p className="max-w-sm">{loadError}</p>
          </div>
        )}
      </div>
      {blocked !== 'none' && !idle && (
        <div className="flex flex-wrap items-center gap-3 bg-biro px-4 py-2.5 text-white" role="status">
          <span className="flex-1 text-[0.9375rem]">
            {blocked === 'muted'
              ? t('Playing without sound because your browser blocked autoplay.')
              : t('Your browser is waiting for a click before it plays video.')}
          </span>
          {blocked === 'muted' ? (
            <Button size="sm" tone="dark" variant="ink" icon={<Volume2 className="size-4" />} onClick={unmute}>
              {t('Turn sound on')}
            </Button>
          ) : (
            <Button size="sm" tone="dark" variant="ink" icon={<Play className="size-4" />} onClick={startManually}>
              {t('Start watching')}
            </Button>
          )}
        </div>
      )}
      {videoError && !idle && (
        <div className="flex flex-wrap items-center gap-3 bg-[#3a1212] px-4 py-2.5 text-[#ffdcdc]" role="alert">
          <span className="flex-1 text-[0.9375rem]">{videoError}</span>
          {canControl ? (
            <Button size="sm" tone="dark" variant="secondary" icon={<SkipForward className="size-4" />} onClick={() => ctrl.queueSkip()}>
              {t('Skip video')}
            </Button>
          ) : (
            <SkipVoteButton />
          )}
        </div>
      )}
      <PlaybackBar playback={playback} duration={playback?.duration ?? localDuration} onResync={apply} />
    </section>
  )
}

function EmptyStage({ canAdd }: { canAdd: boolean }) {
  const t = useT()
  return (
    <div className="absolute inset-0 flex flex-col items-start justify-end gap-2 bg-night-2 p-6 text-fog sm:p-10">
      <p className="display text-[clamp(1.75rem,4vw,3rem)]">{t('Nothing on screen yet')}</p>
      {canAdd && <p className="max-w-md text-haze">{t('Paste a YouTube link in the queue and it starts playing for everyone.')}</p>}
    </div>
  )
}

/** Partello's own controls, outside the YouTube frame. Shows room time, not local time. */
function PlaybackBar({ playback, duration, onResync }: { playback: Playback | null; duration: number | null; onResync: () => void }) {
  const t = useT()
  const ctrl = useController()
  const canControl = useCanControl()
  const [now, setNow] = useState(() => ctrl.clock.now())
  const [scrub, setScrub] = useState<number | null>(null)

  useEffect(() => {
    if (playback?.status !== 'playing') return
    const id = setInterval(() => setNow(ctrl.clock.now()), 250)
    return () => clearInterval(id)
  }, [playback?.status, ctrl])

  const hasItem = !!playback?.item && playback.status !== 'idle'
  const position = hasItem ? expectedPosition(playback, playback.status === 'playing' ? now : playback.updated_at) : 0
  const shown = scrub ?? position
  const max = duration && duration > 0 ? duration : Math.max(position, 1)
  const playing = playback?.status === 'playing'

  const toggle = () => {
    if (!playback) return
    ctrl.control(playing ? 'pause' : 'play', expectedPosition(playback, ctrl.clock.now()))
  }

  const commitScrub = () => {
    if (scrub != null) ctrl.control('seek', scrub)
    setScrub(null)
  }

  return (
    <div className="flex items-center gap-2 bg-night-2 px-2 py-2 sm:gap-3 sm:px-3">
      <IconButton label={playing ? t('Pause for everyone') : t('Play for everyone')} disabled={!hasItem || !canControl} onClick={toggle}>
        {playing ? <Pause className="size-5" /> : <Play className="size-5" />}
      </IconButton>
      <span className="tabular w-[4.5rem] shrink-0 text-right text-small text-haze sm:w-24">
        {formatTime(shown)}
        <span className="hidden sm:inline"> / {duration ? formatTime(duration) : '–:––'}</span>
      </span>
      <input
        type="range"
        aria-label={t('Seek')}
        min={0}
        max={max}
        step={0.5}
        value={Math.min(shown, max)}
        disabled={!hasItem || !canControl || !duration}
        onChange={(e) => setScrub(Number(e.target.value))}
        onPointerUp={commitScrub}
        onKeyUp={commitScrub}
        onBlur={() => setScrub(null)}
        className="h-2 min-w-0 flex-1 cursor-pointer accent-biro-bright disabled:cursor-default disabled:opacity-40"
      />
      {canControl ? (
        <IconButton label={t('Skip to next video')} disabled={!hasItem} onClick={() => ctrl.queueSkip()}>
          <SkipForward className="size-5" />
        </IconButton>
      ) : (
        <SkipVoteButton />
      )}
      <IconButton label={t('Re-sync with the room')} onClick={onResync} disabled={!hasItem}>
        <RefreshCw className="size-[18px]" />
      </IconButton>
      {!canControl && (
        <span className="hidden items-center gap-1.5 pr-1 text-small text-haze md:inline-flex" title={t('The host controls playback')}>
          <Lock className="size-3.5" aria-hidden="true" /> {t('Host controls playback')}
        </span>
      )}
      <span
        className="hidden items-center gap-1.5 pr-1 text-small text-haze xl:inline-flex"
        title={t('YouTube picks the quality for each viewer. Change yours with the gear icon in the player; it does not affect anyone else.')}
      >
        <Settings2 className="size-3.5" aria-hidden="true" /> {t('Quality and subtitles: gear and CC in the player')}
      </span>
    </div>
  )
}
