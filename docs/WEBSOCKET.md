# Partello WebSocket protocol

English · [Bahasa Indonesia](id/WEBSOCKET.md)

Endpoint: `GET /api/rooms/{slug}/ws` (`wss://` in production).

Every frame is a JSON object with a `type` field. The Rust types live in `server/src/realtime/protocol.rs`, the TypeScript types in `web/src/realtime/protocol.ts`; the three (including this document) must always stay in sync.

## Connection flow

1. `POST /api/rooms/{slug}/join` → `session_token` (`pts_…`). The seat is reserved for 60 seconds.
2. Open the WebSocket. The `Origin` header must be in `ALLOWED_ORIGINS`; otherwise the server answers 403.
3. The **first** frame must be `{"type":"auth","token":"pts_…"}`, within 10 seconds.
4. The server answers with `room_state_sync` containing the whole room state. After that events flow both ways.
5. Send `ping` regularly (the Partello client: a burst on connect, then every 10 seconds) to estimate the server clock. The server also sends a WebSocket Ping every 20 seconds and closes connections that are silent for 60 seconds.

## Close codes

| Code | Meaning | Client action |
| --- | --- | --- |
| `4000` | Participant left (`leave`), also sent to the same participant's other tabs | Done |
| `4003` | Removed by the host (`kick`) | Show a message, drop the session |
| `4429` | This session opened a 4th connection; the **oldest** connection is closed (usually a dead socket on a mobile network) | Show "already open in another tab" |
| `4008` | Client reads too slowly (send queue full) | Reconnect |
| `4400` | Too many malformed frames | Reconnect |
| `4401` | Token invalid/expired | Join again, then reconnect |
| `4404` | Room does not exist | Show "room does not exist" |
| `4409` | Room full | Show "room full" |
| other | Network | Reconnect with exponential backoff (0.5 s → 10 s, jitter) |

## Client → server events

| `type` | Fields | Rules |
| --- | --- | --- |
| `auth` | `token` | Only as the first frame |
| `ping` | `t` (client ms) | Answered with `pong` |
| `chat_send` | `body`, `nonce?`, `reply_to?` (message uuid) | 1–500 characters, no control/bidi characters; 6 burst, 1/second. `reply_to` quotes that message; if it is no longer in the in-memory history (last 100 messages), the quote is dropped and the message is sent as a normal one |
| `playback_control` | `action`: `play`/`pause`/`seek`, `position` (seconds) | Host only, or everyone if `control_mode = everyone` |
| `playback_duration` | `item_id`, `duration` | Only from a playback controller; the first report for the active item is accepted |
| `playback_ended` | `item_id` | From a controller, or from anyone if the server agrees the video is nearly over |
| `queue_add` | `url`, `nonce?` | An embeddable YouTube URL; subject to `queue_mode`; max. 100 items |
| `queue_add_playlist` | `url`, `nonce?` | A YouTube link with `list=`; needs `YOUTUBE_API_KEY` (`limits.playlist_import`). Adds up to 50 playable videos, as many as fit in the queue; the first one starts if nothing is playing. YouTube mixes (`list=RD…`) are rejected. Subject to `queue_mode`. Errors (`invalid_video`, `unavailable`, …) echo the `nonce` |
| `queue_remove` | `item_id` | Host, or whoever added the item if `queue_mode = everyone` |
| `queue_play` | `item_id` | Playback controller |
| `queue_move` | `item_id`, `to_index` | Playback controller |
| `queue_skip` | – | Playback controller |
| `skip_vote` | `item_id`, `vote` (bool) | Anyone; `vote: false` takes the vote back. Ignored if `item_id` is no longer the current video. Skips at `floor(n/2)+1` votes from the `n` connected participants |
| `react` | `emoji` | Only one of 😂 🔥 😱 ❤️ 👏 😭 🤯 👀; anything else is `validation_failed` |
| `voice_join` | `muted`, `camera` | Max. `limits.max_voice_participants` people in the call; when full: "The call is full (N people)" |
| `voice_update` | `muted`, `camera` | Must already be in the call |
| `voice_leave` | – | |
| `voice_signal` | `to`, `data` | Both sides in the call; `data.kind` = `description`/`candidate`; max. 24 KB. Only used in mesh mode |
| `host_transfer` | `to` | Host only |
| `kick` | `participant_id` | Host only; the participant is removed, their session revoked, their tabs closed with `4003` |
| `leave` | – | Leave immediately; the session is revoked |

Per-connection rate limits (token bucket): general 30/second, playback 4/second (`playback_control`, `queue_play`, `queue_skip`, `skip_vote`), queue 0.5/second with burst 5 (`queue_add`, `queue_add_playlist`, `queue_remove`, `queue_move`), reactions 3/second with burst 10, signaling 60/second (burst 300, separate from the general budget). One session has at most 3 connections. Violations are answered with `error` with `code: "rate_limited"`.

A participant who loses all their connections is automatically taken out of the call (the call seat is freed). After reconnecting, the client sends `voice_join` again.

## Server → client events

| `type` | Fields | When |
| --- | --- | --- |
| `room_state_sync` | `self_id`, `room`, `participants`, `playback`, `queue`, `chat`, `ice_servers`, `limits`, `skip_votes`, `server_time` | After auth (also after a reconnect). `skip_votes` = ids of who voted to skip the current video |
| `participant_joined` | `participant` | A participant connects for the first time |
| `participant_left` | `participant_id`, `display_name`, `reason`: `left`/`disconnected`/`kicked` | Left, grace window expired, or removed by the host |
| `participant_updated` | `participant` | Host changed (incl. hand-off), connection status, call/mute/camera status |
| `playback_changed` | `playback`, `actor_id`, `server_time`, `nonce?` | Every playback change; `nonce` = the `queue_add`/`queue_add_playlist` nonce it completes. `actor_id` is `null` for changes nobody made directly (e.g. a passed skip vote) |
| `queue_updated` | `queue`, `actor_id`, `nonce?` | Every queue change; `nonce` and `actor_id` as above |
| `chat_message_created` | `message` (the sender's `nonce` is included) | New message |
| `reaction` | `participant_id`, `emoji` | Someone sent a reaction (to everyone, including the sender) |
| `skip_votes` | `item_id`, `voters` (uuid[]), `skipped` | A skip vote was added or taken back. With `skipped: true` the video was skipped and `queue_updated`/`playback_changed` with `actor_id: null` follow |
| `voice_signal` | `from`, `data` | WebRTC signaling, for the recipient only (mesh mode) |
| `call_token` | `url`, `token` | SFU mode only: sent only to the participant whose `voice_join` was accepted. Connect to LiveKit at `url` with `token` |
| `room_settings_changed` | `room` | The host changed settings |
| `pong` | `t`, `server_time` | Answer to `ping` |
| `error` | `code`, `message`, `nonce?` | Only to the sender of the rejected event |

### Main data shapes

```jsonc
// participant
{ "id": "uuid", "display_name": "Ana", "is_host": true, "connected": true,
  "joined_at": 1793113547445, "voice": { "muted": false, "camera": true } | null }

// playback
{ "item": QueueItem | null, "status": "idle|playing|paused|ended",
  "position": 12.5, "updated_at": 1793113547445, "version": 7, "duration": 634 | null }

// queue item
{ "id": "uuid", "video_id": "aqz-KE-bpKQ", "title": "…", "author": "Blender",
  "thumbnail_url": "https://i.ytimg.com/vi/…/hqdefault.jpg", "start": null,
  "added_by": "uuid", "added_by_name": "Ana" }

// chat message (reply_to only on replies, nonce only for the sender)
{ "id": "uuid", "participant_id": "uuid", "display_name": "Budi", "body": "same",
  "created_at": 1793113547445,
  "reply_to": { "id": "uuid", "participant_id": "uuid", "display_name": "Ana",
                "body": "excerpt of at most 140 characters, with … appended when cut" },
  "nonce": "c2" }

// room
{ "slug": "k7Qm2xPa9Rt4", "name": "Movie night",
  "settings": { "control_mode": "host_only", "queue_mode": "everyone", "permanent": false },
  "max_participants": 16, "created_at": 1793113547445, "has_password": false }

// limits
{ "chat_max_chars": 500, "max_voice_participants": 8, "max_queue_len": 100,
  "search_enabled": true, "playlist_import": true, "call_mode": "mesh" | "sfu" }
```

The client computes the current position: `position + (status == "playing" ? (server_now - updated_at)/1000 : 0)`, with `server_now = Date.now() + offset` from ping/pong.

`limits.call_mode` tells the client how to run the call: `mesh` (peer-to-peer, signaling through `voice_signal`) or `sfu` (LiveKit, credentials through `call_token`). `limits.playlist_import` and `limits.search_enabled` are true when the server has `YOUTUBE_API_KEY`.

## Example session

```text
→ {"type":"auth","token":"pts_…"}
← {"type":"room_state_sync","self_id":"…","playback":{"status":"idle",…},"skip_votes":[],…}
→ {"type":"queue_add","url":"https://youtu.be/aqz-KE-bpKQ","nonce":"q1"}
← {"type":"playback_changed","playback":{"status":"playing","position":0,"version":1,…},"actor_id":"…","nonce":"q1"}
→ {"type":"playback_control","action":"pause","position":12.5}
← {"type":"playback_changed","playback":{"status":"paused","position":12.5,"version":2,…}}
→ {"type":"chat_send","body":"popcorn ready","nonce":"c1"}
← {"type":"chat_message_created","message":{"body":"popcorn ready","nonce":"c1",…}}
→ {"type":"react","emoji":"🔥"}
← {"type":"reaction","participant_id":"…","emoji":"🔥"}
→ {"type":"skip_vote","item_id":"…","vote":true}
← {"type":"skip_votes","item_id":"…","voters":["…"],"skipped":false}
```
