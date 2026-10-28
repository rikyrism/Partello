// Peer-to-peer mesh call over WebRTC. Every participant in the call holds one
// RTCPeerConnection per other participant; the Partello server only relays
// signaling (SDP + ICE) and never sees audio or video.
//
// Design choices:
//  * Exactly one side of each pair offers: the participant with the smaller id.
//    The offerer creates an audio and a video transceiver up front, so turning
//    the camera on or off later is a `replaceTrack`, with no renegotiation.
//  * "Perfect negotiation" (polite/impolite roles) still guards against glare
//    from ICE restarts.
//  * `CallTransport` is the seam for swapping the mesh for an SFU (LiveKit,
//    mediasoup) once rooms need more than a handful of cameras.

import type { IceServer, SignalData } from '../../realtime/protocol'
import { videoBitrateFor } from './media'

export interface CallTransport {
  setPeers(ids: string[]): void
  handleSignal(from: string, data: SignalData): Promise<void>
  setMicrophone(track: MediaStreamTrack | null): Promise<void>
  setCamera(track: MediaStreamTrack | null): Promise<void>
  close(): void
}

export interface MeshEvents {
  sendSignal(to: string, data: SignalData): void
  onRemoteStream(peerId: string, stream: MediaStream): void
  onPeerState(peerId: string, state: RTCPeerConnectionState): void
  onPeerClosed(peerId: string): void
}

interface Peer {
  id: string
  pc: RTCPeerConnection
  polite: boolean
  offerer: boolean
  makingOffer: boolean
  ignoreOffer: boolean
  stream: MediaStream
  pendingCandidates: RTCIceCandidateInit[]
}

export class MeshCall implements CallTransport {
  private peers = new Map<string, Peer>()
  private mic: MediaStreamTrack | null = null
  private cam: MediaStreamTrack | null = null
  private closed = false

  constructor(
    private readonly selfId: string,
    private readonly iceServers: IceServer[],
    private readonly events: MeshEvents,
  ) {}

  /** Reconcile peer connections with the current call membership. */
  setPeers(ids: string[]): void {
    if (this.closed) return
    const wanted = new Set(ids.filter((id) => id !== this.selfId))
    for (const id of [...this.peers.keys()]) if (!wanted.has(id)) this.dropPeer(id)
    for (const id of wanted) if (!this.peers.has(id)) this.createPeer(id)
    this.applyBitrate()
  }

  private createPeer(id: string): Peer {
    const pc = new RTCPeerConnection({ iceServers: this.iceServers })
    const offerer = this.selfId < id
    const peer: Peer = {
      id,
      pc,
      offerer,
      polite: !offerer,
      makingOffer: false,
      ignoreOffer: false,
      stream: new MediaStream(),
      pendingCandidates: [],
    }
    this.peers.set(id, peer)

    pc.onicecandidate = ({ candidate }) => {
      this.events.sendSignal(id, { kind: 'candidate', candidate: candidate ? candidate.toJSON() : null })
    }
    pc.ontrack = ({ track }) => {
      if (!peer.stream.getTracks().includes(track)) peer.stream.addTrack(track)
      this.events.onRemoteStream(id, peer.stream)
    }
    pc.onconnectionstatechange = () => {
      this.events.onPeerState(id, pc.connectionState)
      if (pc.connectionState === 'failed' && peer.offerer) pc.restartIce()
    }
    pc.onnegotiationneeded = async () => {
      try {
        peer.makingOffer = true
        await pc.setLocalDescription()
        if (pc.localDescription) this.events.sendSignal(id, { kind: 'description', description: pc.localDescription.toJSON() })
      } catch (err) {
        console.warn('[call] negotiation failed', err)
      } finally {
        peer.makingOffer = false
      }
    }

    if (offerer) {
      // Adding transceivers fires `negotiationneeded`, which sends the offer.
      const audio = pc.addTransceiver('audio', { direction: 'sendrecv', streams: [peer.stream] })
      const video = pc.addTransceiver('video', { direction: 'sendrecv', streams: [peer.stream] })
      void audio.sender.replaceTrack(this.mic)
      void video.sender.replaceTrack(this.cam)
    }
    return peer
  }

  async handleSignal(from: string, data: SignalData): Promise<void> {
    if (this.closed) return
    const peer = this.peers.get(from) ?? this.createPeer(from)
    const { pc } = peer
    try {
      if (data.kind === 'description') {
        const description = data.description
        const collision = description.type === 'offer' && (peer.makingOffer || pc.signalingState !== 'stable')
        peer.ignoreOffer = !peer.polite && collision
        if (peer.ignoreOffer) return
        await pc.setRemoteDescription(description)
        if (description.type === 'offer') {
          await this.attachLocalTracks(peer)
          await pc.setLocalDescription()
          if (pc.localDescription) this.events.sendSignal(from, { kind: 'description', description: pc.localDescription.toJSON() })
        }
        for (const c of peer.pendingCandidates.splice(0)) await pc.addIceCandidate(c)
      } else if (data.candidate) {
        if (!pc.remoteDescription) {
          peer.pendingCandidates.push(data.candidate)
          return
        }
        try {
          await pc.addIceCandidate(data.candidate)
        } catch (err) {
          if (!peer.ignoreOffer) throw err
        }
      }
    } catch (err) {
      console.warn('[call] signal handling failed', err)
    }
  }

  /** The answering side sends on the transceivers the offer created. */
  private async attachLocalTracks(peer: Peer): Promise<void> {
    for (const t of peer.pc.getTransceivers()) {
      const kind = t.receiver.track.kind
      if (t.direction === 'recvonly' || t.direction === 'inactive') t.direction = 'sendrecv'
      await t.sender.replaceTrack(kind === 'audio' ? this.mic : this.cam)
    }
  }

  private async replaceAll(kind: 'audio' | 'video', track: MediaStreamTrack | null): Promise<void> {
    await Promise.all(
      [...this.peers.values()].flatMap((p) =>
        p.pc
          .getTransceivers()
          .filter((t) => t.receiver.track.kind === kind && t.direction !== 'stopped')
          .map((t) => t.sender.replaceTrack(track).catch(() => undefined)),
      ),
    )
  }

  async setMicrophone(track: MediaStreamTrack | null): Promise<void> {
    this.mic = track
    await this.replaceAll('audio', track)
  }

  async setCamera(track: MediaStreamTrack | null): Promise<void> {
    this.cam = track
    await this.replaceAll('video', track)
    this.applyBitrate()
  }

  private applyBitrate(): void {
    const maxBitrate = videoBitrateFor(this.peers.size)
    for (const p of this.peers.values()) {
      for (const sender of p.pc.getSenders()) {
        if (sender.track?.kind !== 'video') continue
        const params = sender.getParameters()
        if (!params.encodings?.length) continue
        params.encodings.forEach((e) => (e.maxBitrate = maxBitrate))
        sender.setParameters(params).catch(() => undefined)
      }
    }
  }

  private dropPeer(id: string): void {
    const peer = this.peers.get(id)
    if (!peer) return
    peer.pc.onicecandidate = null
    peer.pc.ontrack = null
    peer.pc.onconnectionstatechange = null
    peer.pc.onnegotiationneeded = null
    peer.pc.close()
    this.peers.delete(id)
    this.events.onPeerClosed(id)
  }

  close(): void {
    this.closed = true
    for (const id of [...this.peers.keys()]) this.dropPeer(id)
  }
}
