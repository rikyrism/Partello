# Partello architecture

English · [Bahasa Indonesia](id/ARCHITECTURE.md)

## Overview

```
Browser (React)                                Server (Rust, one process)
┌──────────────────────────────┐   HTTPS     ┌────────────────────────────────────────┐
│ Landing / Room / Developers  │────────────▶│ routes/  REST + static SPA             │
│                              │             │   ├─ rooms.rs      (web app)           │
│ RoomController               │  WebSocket  │   └─ developer.rs  (API v1)            │
│  ├─ RoomSocket (auth, ping,  │◀───────────▶│ realtime/ws.rs   1 task per connection │
│  │   reconnect, server clock)│             │ realtime/hub.rs  live rooms + sessions │
│  └─ store (pure reducer)     │             │ room/state.rs    pure rules (no IO)    │
│                              │             │ database.rs      PostgreSQL            │
│ SyncedPlayer ◀── YouTube IFrame Player API │ youtube*.rs      oEmbed + Data API     │
│ MeshCall ◀──── WebRTC P2P ───▶ other peers │ ice.rs           TURN credentials      │
│ SfuCall  ◀──── WebRTC ───▶ LiveKit (opt.)  │ sfu.rs           LiveKit tokens/admin  │
│ sw.js (PWA) ◀── browser push service ◀──   │ push.rs          Web Push (VAPID)      │
└──────────────────────────────┘             └────────────────────────────────────────┘
```

**Modular monolith**: one Rust binary serves REST, the WebSocket, and the built frontend. There are no microservices and no Redis. Both only make sense once there is a concrete need for multiple instances. The one optional external piece is a LiveKit SFU for large calls.

## Backend

| Module | Responsibility |
| --- | --- |
| `config` | Reads environment variables; logs a summary without secrets |
| `room::state` | **All room rules**: presence, host, permissions, playback, queue, skip votes, chat, reactions, voice. Pure, no IO. Every operation returns a list of `Effect`s (broadcast, send to one person, store chat) |
| `room::playback` | Authoritative playback state: `position` at `updated_at` + `status` + `version` |
| `room::queue` | The "up next" queue |
| `realtime::hub` | Owns the live rooms and sessions; turns `Effect`s into WebSocket frames and database writes; saves room state |
| `realtime::ws` | One task per connection: first-frame auth, per-event-type rate limits, heartbeat, idle timeout |
| `realtime::protocol` | Typed client/server event enums (serde, `type` as the tag) |
| `routes` | Thin HTTP handlers: explicit request/response DTOs, logic lives in hub/state |
| `database` | `Store` = enum `Postgres | Memory`, so end-to-end tests run without a database |
| `youtube` | YouTube URL and playlist URL parser, oEmbed for title/thumbnail and an embeddability check |
| `youtube_search` | YouTube Data API v3 (optional), server-side only, cached; also lists playlist videos |
| `ice` | ICE servers for calls: STUN plus TURN from Cloudflare, coturn, or static credentials |
| `sfu` | LiveKit (optional): signs join tokens, removes people from the LiveKit room |
| `push` | Web Push: validates subscriptions (browser push services only), encrypts payloads (RFC 8291 `aes128gcm`), signs requests with VAPID (RFC 8292, ES256). Pure Rust on RustCrypto, no OpenSSL |
| `security` | Random slugs (CSPRNG, rejection sampling), 256-bit tokens, SHA-256 hashes, HMAC TURN credentials |

### Sync model

The server does not run a clock per room. Playback state holds the position at the moment of the last change:

```
current_position = position + (status == playing ? (server_now - updated_at) / 1000 : 0)
```

- Every change bumps `version`; clients ignore older updates.
- Clients estimate the server clock offset with ping/pong (NTP style, lowest-RTT sample).
- Every 500 ms the local player is compared with where it should be. If it is off by > 1 second while playing (0.5 seconds while paused), the player is seeked to the room position, with at least 2.5 seconds between corrections.
- **Loop prevention**: changes made by code open a short "suppress" window. Player events only count as user intent if the YouTube iframe has focus (the user clicked inside it). User seeks are detected from a *jump* in local time between samples, not from the difference to the target, so buffering is not mistaken for a seek.
- Participants without permission who press play/pause/seek in the YouTube player are put straight back to the room state and shown an explanation.
- Playback speed is locked to 1× because it is not shared between viewers.
- Video end: a `playback_ended` report is accepted from a controller, or from a viewer if the server's own estimate also says the video is nearly over. That way viewers cannot skip videos.

### Skip votes

- Anyone in the room can vote to skip the current video (`skip_vote`), and take the vote back.
- The video is skipped when the votes reach a majority of the participants connected right now: `floor(n / 2) + 1`. Only votes from connected participants count, and people who leave the room are removed from the list.
- Votes belong to one video; they reset when the video changes.
- A successful vote is announced with `skip_votes` (`skipped: true`), followed by the usual `queue_updated`/`playback_changed` with `actor_id: null`.
- Hosts (and everyone, when `control_mode = everyone`) still skip directly with `queue_skip`.

### Synchronized start (countdown)

"Start together" lets a group press play at the same moment, with a 3-2-1 everyone sees.

- `start_countdown` comes from someone who may control playback (rate-limited like other playback events), and only while a video is playing or paused; otherwise it is refused with `conflict`.
- The server pauses at the current position (which bumps `version`) and broadcasts `playback_changed`, then `countdown` with `start_at = now + 3000` (`COUNTDOWN_MS`), the paused `version`, and `actor_id`.
- A timer task outside the room lock wakes up after 3 seconds and plays from the same position, with `actor_id: null`, but only if `playback.version` is still the countdown's version and the video is still paused. Any change in between (pause, seek, another video) bumps the version and so cancels the countdown silently.
- Clients draw the big 3-2-1 over the video from `start_at` and their server clock offset, show "Go!" briefly, and drop the overlay as soon as a newer playback version arrives.

### Presence & reconnect

- `POST /join` **reserves a seat** (60 seconds), then the WebSocket authenticates with an `auth` first frame. The token is never in the URL, so it does not end up in proxy logs.
- Connection lost → the participant is marked `connected: false` for a **grace window** (default 45 seconds, enough for a phone switching apps or networks). Reconnecting within that window keeps the same identity with no leave event.
- One participant can have several connections; presence is counted per participant.
- If the server forgot the session (restart), the WebSocket is closed with code `4401` and the client automatically rejoins with the name that tab used.
- Host: the room creator (holder of the host key) always takes the host role back when they join.
- **Host hand-off**: if the host has been disconnected for 10 seconds (`HOST_HANDOFF_MS`), host controls go to the room creator if they are connected, otherwise to the connected member who has been there longest. The original host gets them back on reconnect, unless host was deliberately transferred (`host_transfer`) in the meantime. So the room is never stuck without anyone who can press play.
- If the host leaves, the successor is picked the same way (creator first, then the longest-present member), preferring connected members.

### Concurrency & performance

- One `std::sync::Mutex` per room, only held for synchronous work (state transition + `try_send`). No `await` while the lock is held.
- Broadcast events are serialised **once** and then sent to a bounded channel (256) per connection. Consumers that are too slow are disconnected instead of being allowed to pile up memory.
- oEmbed and playlist lookups (network) happen outside the lock; permissions are checked before and after.
- Limits: HTTP body 16 KB, WebSocket message 64 KB, WebRTC signal 24 KB, chat 500 characters, chat history in memory 100 messages, queue 100, playlist import 50 videos, push subscriptions 200 per room.
- Push notifications are sent from a background task, outside the room lock, so a slow push service never holds up the room.

### Persistent data (PostgreSQL)

| Table | Contents | Why |
| --- | --- | --- |
| `rooms` | slug, name, host key hash, settings (incl. `permanent`), API key owner, `last_active_at`, `saved_state` | Invite links must stay valid after a restart; the video and queue survive it |
| `chat_messages` | chat history per room, incl. the quoted `reply_to` | Latecomers can follow the conversation |
| `api_keys` | label, key **hash**, usage time | Developer API authentication |
| `server_settings` | small server-wide key/value pairs; today the VAPID private key (`vapid_private_key`) | Every push subscription is tied to the server's VAPID key, so it must survive restarts |
| `push_subscriptions` | per room: push endpoint, `p256dh`, `auth`, language (`en`/`id`); unique per room + endpoint | "Someone started watching" notifications for browsers that asked for them |

No email, password, or IP is stored. A push subscription identifies a browser at its push service, not a person. Rooms nobody visited within `ROOM_RETENTION_DAYS` (default 30) are deleted along with their chat, except **permanent** rooms (a host setting, "Keep this room forever"), which are never purged.

#### Saved room state

The current video (playback) and the queue are saved as JSONB in `rooms.saved_state`:

- every 5 seconds, if they changed since the last save;
- before an idle room is unloaded from memory;
- on graceful shutdown (SIGTERM / Ctrl+C).

When a room is loaded again: if the video was playing and the server is back within 2 minutes (`RESUME_WINDOW_MS`), it keeps playing as if the clock had kept running (viewers' players kept going meanwhile). Otherwise it is paused at the position it had at save time.

Sessions are not saved; they live in memory. After a restart clients rejoin silently (with a new participant id), and the creator gets host back through the stored host key.

The migration `20261028000003_room_state.sql` adds `rooms.saved_state JSONB`, `rooms.permanent BOOLEAN`, and `chat_messages.reply_to JSONB`. The migration `20261028000004_push.sql` adds `server_settings` and `push_subscriptions` (deleted with their room).

### Push notifications

A browser can ask to be told when someone starts watching in a room, even with Partello closed (the bell in the room header). This is standard Web Push, implemented in `push.rs` without a third-party push library.

- **When**: someone connects to a room that had nobody connected. Every subscription of that room then gets a notification titled with the room name: "Riky is watching Wildwood. Tap to join." (the video title up to its first `|`), or "Riky started a watch party. Tap to join." when nothing is loaded. The text is in the language chosen when subscribing (`en` or `id`); tapping it opens `/r/{slug}`.
- **How often**: at most one per room every 10 minutes (tracked with the room while it is in memory), and none in the first 2 minutes after the server starts, because everyone is reconnecting then and the room is not really "starting".
- **Encryption** (RFC 8291): one `aes128gcm` record per message, with a fresh ephemeral P-256 key and a random salt each time; the browser's `p256dh` and `auth` keys derive the content key. **Signing** (RFC 8292): an ES256 VAPID JWT with `aud` = the push service origin, a 12-hour `exp`, and `sub` = `VAPID_SUBJECT`. Requests carry `TTL` 6 hours, `Urgency: normal`, and a per-room `Topic` so an undelivered notice is replaced by a newer one.
- **VAPID key**: generated on first start and stored in `server_settings`, or pinned with `VAPID_PRIVATE_KEY`. Browsers subscribe with its public key (`GET /api/push/key`), so changing the key silently ends every existing subscription. With the in-memory store (no `DATABASE_URL`) a new key is made on every start. If the key cannot be loaded, push is disabled with a warning and the push endpoints answer `unavailable`.
- **No SSRF**: a subscription is accepted only if its endpoint is `https` on port 443 at a known browser push service (`fcm.googleapis.com`, `android.googleapis.com`, `push.services.mozilla.com`, `push.apple.com`, `notify.windows.com`, `push.api.chrome.google.com`, or a subdomain of one), at most 1024 characters, with a valid P-256 `p256dh` and a 16-byte `auth`. The server can therefore never be made to call an arbitrary address. Endpoints are capability URLs and are never logged.
- **Cleanup**: a push service answering 404 or 410 means the subscription is gone (unsubscribed, expired), and it is deleted. A room keeps at most 200 subscriptions; past that the oldest are dropped. Subscribing again with the same browser updates its keys and language.
- **Service worker**: shows the notification unless a visible window already has that room open; a tap focuses an open window on the room or opens a new one.

## Frontend

- `RoomController` is the only thing that touches the WebSocket; components use intent-level actions (`control`, `queueAdd`, `sendChat`, …).
- `room/store.ts` holds a pure reducer plus a small external store. Components subscribe to slices of state through `useSyncExternalStore`, so a chat message does not re-render the player.
- `SyncedPlayer` uses the official YouTube IFrame Player API with YouTube's built-in controls (no overlay on top of the iframe, per YouTube policy). Partello's controls sit under the player.
- The room and developer pages are lazy-loaded; the landing page does not load WebRTC/player code. The LiveKit client is only loaded in SFU mode.
- **Reactions**: a reaction bar under the player; the emoji float over the video for everyone. Under `prefers-reduced-motion` they only fade. The pin button next to them posts "📌 m:ss" for the current moment in the chat.
- **Start together**: a timer button in the playback bar (for people who can control playback) sends `start_countdown`; `CountdownOverlay` shows the big 3-2-1 over the video.
- **Theater mode**: a button in the playback bar puts the player area full screen with the Fullscreen API, or in a fixed full-window layout where that API is missing (iPhone). `TheaterOverlay` puts the latest chat messages, a compact chat box, the reactions and the call controls over the video; the chat can be hidden. Esc or the button exits.
- **Chat**: a reply button per message with a quoted block; `@mention` autocomplete (type `@`, then arrows/Tab/Enter); mentions of you are highlighted. Notification sounds are generated with Web Audio: mentions and replies to you always play, other messages only when the chat is not visible. A bell toggle turns them off; the choice is saved per device. A mention or reply while the tab is hidden also shows a system notification, if notification permission was granted earlier (for example through the room's notify bell).
- **Chat timestamps**: times like `12:45` or `1:02:03` in a message are links. For people who can control playback a click seeks the whole room there (an ordinary `playback_control` seek); others see them highlighted. Times past the end of the video stay plain text. This is purely client-side (`features/chat/timestamps.ts`), with no protocol change.
- **Queue**: when a pasted link contains a playlist, a playlist button imports it (only when the server has `YOUTUBE_API_KEY`).
- **Settings**: the host's settings dialog has "Keep this room forever" (`permanent`).
- **i18n** (`web/src/i18n`): text is written in English in the code (`t('Join call')`) and looked up in the `id.ts` dictionary when Indonesian is selected. Error messages from the server are also translated by their text. The test `i18n.test.ts` scans all the code and fails when a string has no translation or its placeholders do not match.
- **PWA**: a web manifest with icons and a service worker (`web/public/sw.js`, registered in production builds only) make Partello installable. Pages are network-first with the cached app shell as the offline fallback; `/assets/` and `/icons/` are cache-first (hashed files never change); `/api/` and the health checks are never cached or intercepted. The landing page shows an "Install app" button when the browser offers installation (`beforeinstallprompt`).
- **Push subscriptions** (`lib/pwa.ts`, `NotifyButton`): the bell in the room header asks for permission, subscribes this browser with the server's VAPID key, and registers it for that room in the interface language. On iPhone and iPad it first explains that Partello has to be added to the Home Screen (iOS 16.4+).
- The landing page follows the design direction from Taste Skill ("page is the object": the invite link as a cinema ticket). Motion uses transform/opacity/clip-path and is turned off under `prefers-reduced-motion`.

## Voice & video (WebRTC)

Calls run in one of two modes, sent to clients as `limits.call_mode`. Both are implementations of the same `CallTransport` interface: `MeshCall` (`web/src/features/call/mesh.ts`) and `SfuCall` (`web/src/features/call/sfu.ts`). The rest of the call UI does not know which one is in use.

### Mesh (default)

- Every call member holds one `RTCPeerConnection` per other member. The server only relays signaling (`voice_signal`) between call members.
- Exactly one side of each pair makes the offer (the smaller id). That side creates audio + video transceivers up front, so **turning the camera on/off is just `replaceTrack`** with no renegotiation. The *perfect negotiation* pattern is still used for ICE restarts.
- Bitrate is capped by the number of peers (900/500/300 kbps); capture is 640×360 @ 24 fps.
- **Mesh limit**: upload grows linearly with the number of participants (8 people with cameras = 7 outgoing streams per person). That is why `ROOM_MAX_VOICE` defaults to 8 in this mode. For larger calls, use the SFU.

### SFU (LiveKit, optional)

Enabled by setting `LIVEKIT_URL`, `LIVEKIT_API_KEY`, and `LIVEKIT_API_SECRET` (plus optionally `LIVEKIT_API_URL`, default `LIVEKIT_URL` with `wss://` → `https://`). Then `call_mode` is `sfu` and `ROOM_MAX_VOICE` defaults to `ROOM_MAX_PARTICIPANTS` instead of 8. Every participant sends their audio and video once, to LiveKit, which forwards it to the others.

Partello stays in charge of who is in the call:

- The server issues a LiveKit token only after it accepted `voice_join`, and sends it only to that participant (`call_token`). The token is an HS256 JWT: `iss` = API key, `sub` = participant id, `name` = display name, and a video grant for room `partello-<room uuid hex>` with `roomJoin`, `canPublish`, `canSubscribe`, `canPublishData: false`, `canPublishSources: ["camera", "microphone"]`. It expires after 10 minutes (it only has to be used to connect). The API secret never leaves the server.
- The LiveKit room name comes from the room's internal id, which is never shown, so knowing the invite link is not enough to find the call.
- When someone leaves the Partello room for any reason (left, grace window expired, kicked) or sends `voice_leave`, the server removes them from the LiveKit room through the Twirp API `livekit.RoomService/RemoveParticipant`.
- The frontend loads `livekit-client` on demand, only in SFU mode. `adaptiveStream` is off because video is rendered in Partello's own elements, `dynacast` is on, and the camera is published with simulcast.
- Mute and push-to-talk work the same as in the mesh: the microphone track is disabled.
- The CSP `connect-src` includes the LiveKit origin (both `wss://` and `https://`).

### Shared by both modes

- Microphone permission is requested only when pressing "Join call"; camera permission only when turning the camera on.
- The speaking indicator is computed locally from the audio level (WebAudio), with no extra traffic.
- **Video audio leaking into the microphone.** Browser echo cancellation only removes call audio, not the YouTube iframe audio coming out of the same speakers. Layered mitigations: (1) the constraint `echoCancellation: 'all'` (cancels all system audio in recent Chromium; older browsers read it as `true`) and `voiceIsolation`; (2) **auto-ducking**: video volume drops to 65% during a call and to 20% while someone is talking, then comes back; (3) **push-to-talk** (hold the button/Space), the microphone is only live while held; (4) a headset recommendation in the UI.
- All tracks, peer connections, and listeners are cleaned up when leaving the call, leaving the room, or on unmount.

### TURN

STUN alone connects most home networks. Phones on mobile data (CGNAT) and strict office networks need a TURN relay. The server uses the first source that is configured:

1. **Cloudflare Realtime TURN** (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`): the server POSTs to Cloudflare's `generate-ice-servers` endpoint with a 6-hour TTL, caches the result for 3 hours, and drops port-53 URLs. The credentials are shared by all participants. If the request fails, clients get STUN only and the server logs a warning.
2. **coturn shared secret** (`TURN_URLS` + `TURN_SHARED_SECRET`, coturn `use-auth-secret` style): the server mints short-lived credentials per participant. The secret is never sent to the browser.
3. **Static credentials** (`TURN_URLS` + `TURN_USERNAME` + `TURN_CREDENTIAL`, e.g. a free Metered.ca account). These are visible to room members.

## Scale and distributed state

Consequences of in-memory state:

1. **A server restart** drops presence, sessions, and call status. Rooms, chat, the current video, and the queue stay in PostgreSQL (the video and queue are saved every 5 seconds and on shutdown, see [Saved room state](#saved-room-state)); clients rejoin automatically.
2. **Multiple instances** need all connections of a room to land on the same instance. Simplest option: sticky routing by slug in the load balancer.

Migration path when needed:

- `room::state` is already pure and serialisable. Room state can move to Redis (or be held by one "room owner" per room) without changing the rules.
- `Hub::dispatch` is the only place events are sent out. Cross-instance fan-out only needs to be added there (e.g. Redis pub/sub per room).
- Sessions can move to a PostgreSQL or Redis table.
- Per-IP/key rate limits need shared storage (Redis) or should move to the edge/proxy.
