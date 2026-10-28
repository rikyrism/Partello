// Call state for the room page: device tracks, the mesh, remote streams.
// Microphone permission is requested only when the user chooses to join.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useController, useRoom } from '../../room/context'
import { useToast } from '../../components/Toast'
import { translate } from '../../i18n'
import { DeviceError, getCamera, getMicrophone } from './media'
import { MeshCall } from './mesh'
import { audioContext, useAnySpeaking } from './speaking'

type Status = 'idle' | 'joining' | 'joined'

export interface CallApi {
  status: Status
  muted: boolean
  camera: boolean
  cameraBusy: boolean
  localStream: MediaStream | null
  remoteStreams: Record<string, MediaStream>
  peerStates: Record<string, RTCPeerConnectionState>
  volumes: Record<string, number>
  join(withCamera: boolean): Promise<void>
  leave(): void
  toggleMute(): void
  toggleCamera(): Promise<void>
  setVolume(peerId: string, volume: number): void
  /** Only send audio while the talk key/button is held. */
  pushToTalk: boolean
  setPushToTalk(on: boolean): void
  talkHeld: boolean
  setTalkHeld(held: boolean): void
  /** Lower the video while anyone in the call is talking. */
  autoDuck: boolean
  setAutoDuck(on: boolean): void
  /** Someone is talking right now and auto-duck is on. */
  ducking: boolean
}

const CallContext = createContext<CallApi | null>(null)

export function useCall(): CallApi {
  const call = useContext(CallContext)
  if (!call) throw new Error('useCall outside CallProvider')
  return call
}

export function CallProvider({ children }: { children: ReactNode }) {
  const ctrl = useController()
  const toast = useToast()
  const selfId = useRoom((s) => s.selfId)
  const participants = useRoom((s) => s.participants)
  const iceServers = useRoom((s) => s.iceServers)
  const synced = useRoom((s) => s.synced)
  const connection = useRoom((s) => s.status)

  const [status, setStatus] = useState<Status>('idle')
  const [muted, setMuted] = useState(false)
  const [camera, setCamera] = useState(false)
  const [cameraBusy, setCameraBusy] = useState(false)
  const [localStream, setLocalStream] = useState<MediaStream | null>(null)
  const [remoteStreams, setRemoteStreams] = useState<Record<string, MediaStream>>({})
  const [peerStates, setPeerStates] = useState<Record<string, RTCPeerConnectionState>>({})
  const [volumes, setVolumes] = useState<Record<string, number>>({})
  const [pushToTalk, setPushToTalk] = useState(false)
  const [talkHeld, setTalkHeld] = useState(false)
  const [autoDuck, setAutoDuck] = useState(true)

  const meshRef = useRef<MeshCall | null>(null)
  const micRef = useRef<MediaStreamTrack | null>(null)
  const camRef = useRef<MediaStreamTrack | null>(null)
  const stateRef = useRef({ muted, camera })
  useEffect(() => {
    stateRef.current = { muted, camera }
  }, [muted, camera])

  const rebuildLocalStream = useCallback(() => {
    const tracks = [micRef.current, camRef.current].filter((t): t is MediaStreamTrack => !!t)
    setLocalStream(tracks.length ? new MediaStream(tracks) : null)
  }, [])

  const voiceIds = useMemo(
    () => participants.filter((p) => p.voice && p.id !== selfId).map((p) => p.id).sort().join(','),
    [participants, selfId],
  )
  const selfVoice = participants.find((p) => p.id === selfId)?.voice ?? null

  // Bumped by every teardown so an in-flight join can tell it was cancelled
  // (user left, page unmounted) while it awaited the microphone.
  const joinGen = useRef(0)
  // True between sending voice_join and seeing ourselves in the call.
  const awaitingServer = useRef(false)

  const teardown = useCallback(() => {
    joinGen.current += 1
    awaitingServer.current = false
    meshRef.current?.close()
    meshRef.current = null
    micRef.current?.stop()
    camRef.current?.stop()
    micRef.current = null
    camRef.current = null
    setLocalStream(null)
    setRemoteStreams({})
    setPeerStates({})
    setCamera(false)
    setMuted(false)
    setTalkHeld(false)
    setStatus('idle')
  }, [])

  const stopCamera = useCallback(async () => {
    camRef.current?.stop()
    camRef.current = null
    await meshRef.current?.setCamera(null)
    setCamera(false)
    rebuildLocalStream()
    ctrl.voiceUpdate(stateRef.current.muted, false)
  }, [ctrl, rebuildLocalStream])

  const startCamera = useCallback(async (): Promise<boolean> => {
    try {
      const track = await getCamera()
      track.onended = () => {
        toast(translate('Your camera was disconnected'))
        void stopCamera()
      }
      camRef.current = track
      await meshRef.current?.setCamera(track)
      setCamera(true)
      rebuildLocalStream()
      return true
    } catch (err) {
      toast(err instanceof DeviceError ? err.message : translate("Couldn't start your camera."), 'error')
      return false
    }
  }, [toast, rebuildLocalStream, stopCamera])

  const join = useCallback(
    async (withCamera: boolean) => {
      if (status !== 'idle' || !selfId) return
      const gen = joinGen.current
      setStatus('joining')
      audioContext()
      let mic: MediaStreamTrack
      try {
        mic = await getMicrophone()
      } catch (err) {
        if (gen === joinGen.current) {
          toast(err instanceof DeviceError ? err.message : translate("Couldn't start your microphone."), 'error')
          setStatus('idle')
        }
        return
      }
      if (gen !== joinGen.current) {
        mic.stop() // cancelled while the permission prompt was open
        return
      }
      mic.onended = () => {
        toast(translate('Your microphone was disconnected'), 'error')
        setMuted(true)
      }
      micRef.current = mic
      const mesh = new MeshCall(selfId, iceServers, {
        sendSignal: (to, data) => ctrl.sendSignal(to, data),
        onRemoteStream: (id, stream) => setRemoteStreams((s) => ({ ...s, [id]: stream })),
        onPeerState: (id, state) => setPeerStates((s) => ({ ...s, [id]: state })),
        onPeerClosed: (id) => {
          setRemoteStreams(({ [id]: _gone, ...rest }) => rest)
          setPeerStates(({ [id]: _gone, ...rest }) => rest)
        },
      })
      meshRef.current = mesh
      await mesh.setMicrophone(mic)
      let cameraOn = false
      if (withCamera) cameraOn = await startCamera()
      if (gen !== joinGen.current) {
        // Cancelled while the camera prompt was open: release it again.
        camRef.current?.stop()
        camRef.current = null
        setCamera(false)
        setLocalStream(null)
        return
      }
      rebuildLocalStream()
      setMuted(false)
      if (!ctrl.voiceJoin(false, cameraOn)) {
        toast(translate('Not connected. Try again in a moment.'), 'error')
        teardown()
        return
      }
      // Stay in "joining" until the server lists us in the call (see below);
      // a rejection (call full) tears everything down again.
      awaitingServer.current = true
    },
    [status, selfId, iceServers, ctrl, toast, startCamera, rebuildLocalStream, teardown],
  )

  const leave = useCallback(() => {
    if (status === 'idle') return
    ctrl.voiceLeave()
    teardown()
  }, [status, ctrl, teardown])

  const toggleMute = useCallback(() => {
    if (!micRef.current) return
    const next = !muted
    setMuted(next)
    ctrl.voiceUpdate(next, camera)
  }, [muted, camera, ctrl])

  // The microphone sends only when unmuted and, in push-to-talk, while held.
  useEffect(() => {
    if (micRef.current) micRef.current.enabled = !muted && (!pushToTalk || talkHeld)
  }, [muted, pushToTalk, talkHeld, localStream])

  // Hold Space to talk (unless typing). Releasing or leaving the window stops it.
  useEffect(() => {
    if (!pushToTalk || status !== 'joined') return
    const typing = (t: EventTarget | null) =>
      t instanceof HTMLElement && (t.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName))
    const down = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !e.repeat && !typing(e.target)) {
        e.preventDefault()
        setTalkHeld(true)
      }
    }
    const up = (e: KeyboardEvent) => {
      if (e.code === 'Space') setTalkHeld(false)
    }
    const release = () => setTalkHeld(false)
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    window.addEventListener('blur', release)
    return () => {
      window.removeEventListener('keydown', down)
      window.removeEventListener('keyup', up)
      window.removeEventListener('blur', release)
    }
  }, [pushToTalk, status])

  const duckSources = useMemo(
    () => (status === 'joined' ? [localStream, ...Object.values(remoteStreams)].filter((s): s is MediaStream => !!s) : []),
    [status, localStream, remoteStreams],
  )
  const ducking = useAnySpeaking(duckSources, autoDuck && status === 'joined')

  const toggleCamera = useCallback(async () => {
    if (status !== 'joined' || cameraBusy) return
    setCameraBusy(true)
    try {
      if (camera) await stopCamera()
      else if (await startCamera()) ctrl.voiceUpdate(muted, true)
    } finally {
      setCameraBusy(false)
    }
  }, [status, cameraBusy, camera, muted, ctrl, startCamera, stopCamera])

  const setVolume = useCallback((peerId: string, volume: number) => {
    setVolumes((v) => ({ ...v, [peerId]: volume }))
  }, [])

  // Relay incoming signaling to the mesh.
  useEffect(() => ctrl.onSignal((from, data) => void meshRef.current?.handleSignal(from, data)), [ctrl])

  // Keep one peer connection per call member.
  useEffect(() => {
    if (status === 'joined') meshRef.current?.setPeers(voiceIds ? voiceIds.split(',') : [])
  }, [status, voiceIds])

  // The server confirmed the join: now we are in the call.
  useEffect(() => {
    if (status === 'joining' && awaitingServer.current && selfVoice) {
      awaitingServer.current = false
      setStatus('joined')
    }
  }, [status, selfVoice])

  // The server refused the join (e.g. the call filled up a moment earlier).
  useEffect(
    () =>
      ctrl.onServerError((code, message) => {
        // Only "the call is full" refuses a join; other conflicts are unrelated.
        if (awaitingServer.current && code === 'conflict' && message.startsWith('The call is full')) teardown()
      }),
    [ctrl, teardown],
  )

  // Never wait forever for a confirmation.
  useEffect(() => {
    if (status !== 'joining') return
    const id = setTimeout(() => {
      if (awaitingServer.current) {
        toast(translate('Not connected. Try again in a moment.'), 'error')
        ctrl.voiceLeave()
        teardown()
      }
    }, 10_000)
    return () => clearTimeout(id)
  }, [status, ctrl, toast, teardown])

  // The server drops a connection-less participant from the call; after we
  // reconnect, announce ourselves again so peers reconnect to us.
  const prevConnection = useRef(connection)
  useEffect(() => {
    const was = prevConnection.current
    prevConnection.current = connection
    if (!synced || connection !== 'open' || was === 'open') return
    if (status === 'joined' && !selfVoice) {
      // Peers dropped their connections to us when the server cleared our
      // seat; start fresh ones instead of reusing stale peer connections.
      queueMicrotask(() => {
        meshRef.current?.setPeers([])
        awaitingServer.current = true
        setStatus('joining')
        ctrl.voiceJoin(stateRef.current.muted, stateRef.current.camera)
      })
    }
  }, [synced, connection, status, selfVoice, ctrl])

  // Release devices when leaving the room page.
  useEffect(() => teardown, [teardown])

  const api = useMemo<CallApi>(
    () => ({
      status,
      muted,
      camera,
      cameraBusy,
      localStream,
      remoteStreams,
      peerStates,
      volumes,
      join,
      leave,
      toggleMute,
      toggleCamera,
      setVolume,
      pushToTalk,
      setPushToTalk,
      talkHeld,
      setTalkHeld,
      autoDuck,
      setAutoDuck,
      ducking,
    }),
    [
      status,
      muted,
      camera,
      cameraBusy,
      localStream,
      remoteStreams,
      peerStates,
      volumes,
      join,
      leave,
      toggleMute,
      toggleCamera,
      setVolume,
      pushToTalk,
      talkHeld,
      autoDuck,
      ducking,
    ],
  )

  return <CallContext.Provider value={api}>{children}</CallContext.Provider>
}
