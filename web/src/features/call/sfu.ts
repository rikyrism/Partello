// Calls through a LiveKit SFU: every participant sends their audio and video
// once, to the server, which forwards it to everyone else. That scales to far
// more people than the mesh, at the cost of a media server.
//
// Partello still decides who is in the call: the room server hands out a
// LiveKit token only after accepting `voice_join`, and removes people from the
// LiveKit room when they are kicked or leave. This module is loaded on demand,
// so rooms that use the mesh never download the LiveKit client.

import {
  ConnectionState,
  DisconnectReason,
  type LocalTrackPublication,
  type Participant,
  type RemoteTrack,
  Room,
  RoomEvent,
  Track,
} from 'livekit-client'
import type { CallTransport, MeshEvents } from './mesh'
import { videoBitrateFor } from './media'

export interface SfuEvents extends MeshEvents {
  /** The SFU connection ended without us leaving (network gone, removed). */
  onDisconnected(): void
}

export class SfuCall implements CallTransport {
  private room: Room | null = null
  private mic: MediaStreamTrack | null = null
  private cam: MediaStreamTrack | null = null
  private micPub: LocalTrackPublication | null = null
  private camPub: LocalTrackPublication | null = null
  private streams = new Map<string, MediaStream>()
  private closed = false

  constructor(private readonly events: SfuEvents) {}

  /** LiveKit tracks call membership itself. */
  setPeers(): void {}

  async handleSignal(): Promise<void> {}

  /** Connects once; tokens that arrive again after a reconnect are ignored. */
  async connect(url: string, token: string): Promise<void> {
    if (this.closed || this.room) return
    // Video is shown from our own <video> elements, so LiveKit cannot see
    // their size: keep every subscribed video flowing (adaptiveStream off).
    const room = new Room({ adaptiveStream: false, dynacast: true })
    this.room = room

    room
      .on(RoomEvent.ParticipantConnected, (p) => this.events.onPeerState(p.identity, 'connected'))
      .on(RoomEvent.ParticipantDisconnected, (p) => this.dropPeer(p.identity))
      .on(RoomEvent.TrackSubscribed, (track, _pub, p) => this.addTrack(p, track))
      .on(RoomEvent.TrackUnsubscribed, (track, _pub, p) => this.removeTrack(p, track))
      .on(RoomEvent.Reconnecting, () => this.allPeers('connecting'))
      .on(RoomEvent.Reconnected, () => this.allPeers('connected'))
      .on(RoomEvent.Disconnected, (reason?: DisconnectReason) => {
        if (this.closed) return
        console.warn('[call] sfu disconnected', reason)
        this.events.onDisconnected()
      })

    await room.connect(url, token, { autoSubscribe: true })
    if (this.closed) {
      void room.disconnect()
      return
    }
    for (const p of room.remoteParticipants.values()) {
      this.events.onPeerState(p.identity, 'connected')
      for (const pub of p.trackPublications.values()) {
        if (pub.track) this.addTrack(p, pub.track as RemoteTrack)
      }
    }
    await this.publishMic()
    await this.publishCam()
  }

  private stream(identity: string): MediaStream {
    let s = this.streams.get(identity)
    if (!s) {
      s = new MediaStream()
      this.streams.set(identity, s)
    }
    return s
  }

  private addTrack(p: Participant, track: RemoteTrack): void {
    const s = this.stream(p.identity)
    if (!s.getTracks().includes(track.mediaStreamTrack)) s.addTrack(track.mediaStreamTrack)
    this.events.onRemoteStream(p.identity, s)
    this.events.onPeerState(p.identity, 'connected')
  }

  private removeTrack(p: Participant, track: RemoteTrack): void {
    const s = this.streams.get(p.identity)
    if (!s) return
    s.removeTrack(track.mediaStreamTrack)
    this.events.onRemoteStream(p.identity, s)
  }

  private dropPeer(identity: string): void {
    this.streams.delete(identity)
    this.events.onPeerClosed(identity)
  }

  private allPeers(state: RTCPeerConnectionState): void {
    for (const p of this.room?.remoteParticipants.values() ?? []) this.events.onPeerState(p.identity, state)
  }

  private get connected(): boolean {
    return this.room?.state === ConnectionState.Connected
  }

  private async publishMic(): Promise<void> {
    const local = this.room?.localParticipant
    if (!local || !this.connected) return
    if (this.micPub?.track) await local.unpublishTrack(this.micPub.track, false)
    this.micPub = this.mic ? await local.publishTrack(this.mic, { source: Track.Source.Microphone }) : null
  }

  private async publishCam(): Promise<void> {
    const local = this.room?.localParticipant
    if (!local || !this.connected) return
    if (this.camPub?.track) await local.unpublishTrack(this.camPub.track, false)
    this.camPub = this.cam
      ? await local.publishTrack(this.cam, {
          source: Track.Source.Camera,
          simulcast: true,
          videoEncoding: { maxBitrate: videoBitrateFor(1), maxFramerate: 30 },
        })
      : null
  }

  async setMicrophone(track: MediaStreamTrack | null): Promise<void> {
    this.mic = track
    await this.publishMic().catch((err) => console.warn('[call] publishing the microphone failed', err))
  }

  async setCamera(track: MediaStreamTrack | null): Promise<void> {
    this.cam = track
    await this.publishCam().catch((err) => console.warn('[call] publishing the camera failed', err))
  }

  close(): void {
    this.closed = true
    const room = this.room
    this.room = null
    // Our own tracks belong to the call provider, which stops them.
    if (room) void room.disconnect(false)
    for (const id of [...this.streams.keys()]) this.dropPeer(id)
  }
}
