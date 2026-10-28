// Loads the official YouTube IFrame Player API once and exposes the small
// subset of its types we use. Video data never passes through Partello.

export const YTState = {
  UNSTARTED: -1,
  ENDED: 0,
  PLAYING: 1,
  PAUSED: 2,
  BUFFERING: 3,
  CUED: 5,
} as const

export interface YTPlayer {
  playVideo(): void
  pauseVideo(): void
  seekTo(seconds: number, allowSeekAhead: boolean): void
  loadVideoById(opts: { videoId: string; startSeconds?: number }): void
  cueVideoById(opts: { videoId: string; startSeconds?: number }): void
  getCurrentTime(): number
  getDuration(): number
  getPlayerState(): number
  mute(): void
  unMute(): void
  isMuted(): boolean
  getIframe(): HTMLIFrameElement
  getVolume(): number
  setVolume(volume: number): void
  getPlaybackRate(): number
  setPlaybackRate(rate: number): void
  destroy(): void
}

interface YTEvent {
  target: YTPlayer
  data: number
}

export interface YTPlayerOptions {
  videoId?: string
  host?: string
  width?: string | number
  height?: string | number
  playerVars?: Record<string, string | number>
  events?: {
    onReady?: (e: YTEvent) => void
    onStateChange?: (e: YTEvent) => void
    onError?: (e: YTEvent) => void
    onAutoplayBlocked?: (e: YTEvent) => void
    onPlaybackRateChange?: (e: YTEvent) => void
  }
}

interface YTNamespace {
  Player: new (el: HTMLElement, opts: YTPlayerOptions) => YTPlayer
}

declare global {
  interface Window {
    YT?: YTNamespace
    onYouTubeIframeAPIReady?: () => void
  }
}

let loading: Promise<YTNamespace> | null = null

export function loadYouTubeApi(): Promise<YTNamespace> {
  if (window.YT?.Player) return Promise.resolve(window.YT)
  if (loading) return loading
  loading = new Promise((resolve, reject) => {
    const previous = window.onYouTubeIframeAPIReady
    window.onYouTubeIframeAPIReady = () => {
      previous?.()
      if (window.YT) resolve(window.YT)
    }
    const script = document.createElement('script')
    script.src = 'https://www.youtube.com/iframe_api'
    script.async = true
    script.onerror = () => {
      loading = null
      reject(new Error("Couldn't load the YouTube player. Check your connection or content blockers."))
    }
    document.head.appendChild(script)
  })
  return loading
}
