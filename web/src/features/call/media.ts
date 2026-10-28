// Device access with human-readable failures.

import { translate } from '../../i18n'

/**
 * Microphone processing. Browser echo cancellation normally removes only the
 * call's own audio, not the YouTube video playing from the same speakers, so
 * we also ask for:
 *  - `echoCancellation: 'all'`: cancel all system playback where supported
 *    (newer Chromium); older browsers coerce the string to `true`.
 *  - `voiceIsolation`: speech enhancement that suppresses music and effects
 *    (ignored where unsupported).
 * Headphones remain the only complete fix; the UI says so.
 */
export const AUDIO_CONSTRAINTS = {
  echoCancellation: 'all',
  noiseSuppression: true,
  autoGainControl: true,
  voiceIsolation: true,
  channelCount: 1,
} as unknown as MediaTrackConstraints

/** Modest capture size: in a mesh every viewer receives every camera. */
export const VIDEO_CONSTRAINTS: MediaTrackConstraints = {
  width: { ideal: 640 },
  height: { ideal: 360 },
  frameRate: { ideal: 24, max: 30 },
  facingMode: 'user',
}

export class DeviceError extends Error {}

export function describeMediaError(err: unknown, device: 'microphone' | 'camera'): string {
  const name = err instanceof DOMException ? err.name : ''
  const vars = { device: translate(device) }
  switch (name) {
    case 'NotAllowedError':
    case 'SecurityError':
      return translate("Access to your {device} was blocked. Allow it in your browser's site settings, then try again.", vars)
    case 'NotFoundError':
    case 'OverconstrainedError':
      return translate('No {device} was found on this device.', vars)
    case 'NotReadableError':
    case 'AbortError':
      return translate('Your {device} is being used by another app. Close it and try again.', vars)
    default:
      return translate("Couldn't start your {device}.", vars)
  }
}

function assertSupported() {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new DeviceError(translate('Calls need a secure (https) page and a full browser. If you opened this inside WhatsApp, Instagram or another app, open it in Chrome or Safari.'))
  }
}

const BASIC_AUDIO: MediaTrackConstraints = { echoCancellation: true, noiseSuppression: true, autoGainControl: true }

export async function getMicrophone(): Promise<MediaStreamTrack> {
  assertSupported()
  try {
    const stream = await navigator.mediaDevices
      .getUserMedia({ audio: AUDIO_CONSTRAINTS, video: false })
      .catch((err: unknown) => {
        // A browser that rejects the newer constraints gets the classic set.
        if (err instanceof TypeError || (err instanceof DOMException && err.name === 'OverconstrainedError')) {
          return navigator.mediaDevices.getUserMedia({ audio: BASIC_AUDIO, video: false })
        }
        throw err
      })
    return stream.getAudioTracks()[0]!
  } catch (err) {
    throw new DeviceError(describeMediaError(err, 'microphone'))
  }
}

export async function getCamera(): Promise<MediaStreamTrack> {
  assertSupported()
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: false, video: VIDEO_CONSTRAINTS })
    return stream.getVideoTracks()[0]!
  } catch (err) {
    throw new DeviceError(describeMediaError(err, 'camera'))
  }
}

/** Per-sender video bitrate cap so total upload stays reasonable as the mesh grows. */
export function videoBitrateFor(peerCount: number): number {
  if (peerCount <= 1) return 900_000
  if (peerCount <= 3) return 500_000
  return 300_000
}
