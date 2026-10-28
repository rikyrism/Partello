# Testing Guide

English · [Bahasa Indonesia](id/TESTING.md)

## Automated tests

### Backend (`server/`)

```bash
cargo test                                   # everything: 70 unit + 32 end-to-end (in-memory store)
cargo test --lib                             # unit tests only (70)
TEST_DATABASE_URL=postgres://partello:partello@localhost:5433/partello cargo test --test postgres   # 1 PostgreSQL store test
cargo test --lib push::tests::vector -- --ignored --nocapture   # ignored helper: prints a Web Push test vector
cargo test --lib push::live -- --ignored --nocapture            # ignored helper: live request to Google FCM (needs internet)
```

| Scope | Location | What is tested |
| --- | --- | --- |
| Room state machine | `src/room/state_tests.rs` | First member becomes host, capacity, invisible reservations, creator reclaims host, reconnect grace window, multiple tabs, host-only playback permission, invalid positions, queue & its permissions, queue capacity, validated `ended` reports, chat ring buffer, voice limits & signaling, host transfer, kicks, reactions limited to the allowed set, skip votes (majority skips, people who left stop counting), replies quote the original, host controls move to someone still connected after the host is away and come back when they return (a deliberate hand-over is not undone), save and restore of video + queue, playlists fill the queue up to capacity, permanent room as a host setting, the countdown pauses then plays from the same spot, a change during the countdown cancels it |
| Playback | `src/room/playback.rs` | play/pause/seek/load/finish transitions, position clamping, monotonic versions, duration |
| Queue | `src/room/queue.rs` | Capacity, remove, pop, reorder |
| Validation | `src/validation.rs`, `src/youtube.rs`, `src/youtube_search.rs` | Names, chat, bidi/control characters, YouTube URL shapes, playlist links (YouTube mixes rejected), Data API response parsing for search and playlists (order kept, unplayable videos dropped), playlist import disabled without a key |
| Protocol | `src/realtime/protocol.rs` | Valid events & malformed payloads rejected |
| Security | `src/security.rs`, `src/rate_limit.rs` | Slugs, hashes, TURN credential formula (checked against `openssl`), token bucket, `X-Forwarded-For` |
| ICE / TURN | `src/ice.rs` | Cloudflare TURN response parsing (STUN entries and port 53 dropped), static credentials passed through, coturn secret wins over static credentials |
| SFU | `src/sfu.rs` | LiveKit join tokens scoped to one call (room, `roomJoin`, camera + microphone only, no admin, short expiry), HS256 signature |
| Web Push | `src/push.rs`, `src/realtime/hub.rs` | Payloads decrypt with an independent RFC 8291 receiver written from the spec, fresh salt and sender key per message, subscriptions limited to browser push services (http, look-alike hosts, IP addresses, other ports and bad keys refused), VAPID JWT signed for the push service origin and verifiable with the public key, the stored key restores the same identity, `Topic` is URL-safe and short, notification text in the subscriber's language |
| **End-to-end** | `tests/realtime.rs` | Real server + 2–3 WebSocket clients: synced playback/queue/chat, late joiner receives current state, reconnect within grace, leaving after grace, immediate leave, host promotion, malformed payloads, chat rate limit, signaling relay, fake auth & foreign origin, REST validation & full room, settings permission, developer API, idle room unload & retention purge, health, search without a key, PIN rooms, kicks, reactions and skip votes reach everyone, chat replies carry the quote, video and queue survive a restart (two server instances sharing one memory store), playlist import without a key, SFU tokens only for call members, CSP lists the SFU origin, permanent rooms, the countdown (guests refused in a host-only room, pause → `countdown` → play for everyone), push subscriptions (key endpoint 503 until push is set up, session required, only browser push services, unsubscribe) |
| PostgreSQL | `tests/postgres.rs` | Migrations, rooms, recent chat with replies, saved room state, API keys, cascade delete, permanent rooms survive the retention purge, server settings keep the first value written, push subscriptions (subscribing again updates in place, removal) |

### Frontend (`web/`)

```bash
npm test          # Vitest + Testing Library (jsdom), 59 tests
npm run lint
npm run typecheck
```

| File | What is tested |
| --- | --- |
| `syncMath.test.ts` | Expected position, jump vs buffering detection, drift correction decisions, unauthorized user seeks reverted |
| `store.test.ts` | Snapshot, join/leave/call/camera system messages, stale playback updates ignored, optimistic chat reconciliation |
| `parsing.test.ts` | YouTube link parser, room code, server clock estimate, time formatting |
| `features.test.ts` | @mentions (case-insensitive, word boundary, longest name first, name being typed), playlist link detection (mixes excluded), skip votes (majority of people present, votes for the current video, reset when the video changes) |
| `invite.test.ts` | Invite message in English and Indonesian, with the PIN or who to ask for it |
| `components.test.tsx` | Chat (Enter sends, Shift+Enter newline, empty/reconnecting state), queue (link validation, YouTube search through the server, host-only mode), join gate (name validation, PIN, full room) |
| `timestamps.test.ts` | Chat timestamps (`m:ss` and `h:mm:ss` found, other numbers, times and versions left alone), countdown in the store (starts on the paused version, ends with the next playback change, ignored when playback already moved on) |
| `i18n.test.ts` | Every `t()`/`translate()`/`msg()` in the code has an Indonesian translation, genre names & chat system notes complete, `{…}` placeholders consistent, fallback to English |

### Web Push encryption against other implementations

The unit tests decrypt with a receiver written in this repo, so the encryption was also checked against outside code:

- **`http_ece`** (an independent Node implementation of the same encryption, used by the `web-push` package): the vector printed by `push::tests::vector` (browser private key, `p256dh`, `auth`, encrypted body) decrypts to `hello from partello`.
- **Google FCM**: `push::live` sends a VAPID-signed, encrypted request to a made-up FCM subscription. FCM answers **410** (the subscription does not exist): the request got past VAPID authentication.

## Manual test: two browsers, synced playback

Run during development in Chrome with two tabs (host & guest).

1. Start the server (`cargo run`) and open `http://localhost:8080`.
2. **Browser A**: write a name on the ticket → *Create a room*. The room opens with status *Live*.
3. Click *Invite* → the link is copied.
4. **Browser B** (an incognito window or another browser, so localStorage is separate): open the link → enter a name → *Join the room*.
   - A sees "B joined" in chat and the *People* count goes up.
   - B sees the label "Host controls playback".
5. A: *Queue* tab → paste `https://www.youtube.com/watch?v=aqz-KE-bpKQ` → *Add*. The video plays in both browsers.
6. A: pause button under the player → both browsers stop at the same second.
7. B: click the video to play → refused, the video returns to the room position, and a toast shows "Only the host can control playback".
8. A: drag the seek slider to the middle → B jumps to the same position (≤ 1 second apart).
9. B: send a chat message → it appears in A in real time.
10. B: reload the page → back as the same participant, video continues at the room position.
11. Turn B's Wi-Fi off for 5 seconds and back on → "Reconnecting" indicator, then reconnected without a "left" message.

## Manual test: room features

Use two or three devices (or browsers with separate localStorage) in the same room.

| # | Scenario | Expected |
| --- | --- | --- |
| 1 | Reactions from two devices | Each reaction appears on both screens at about the same time |
| 2 | Vote to skip with 3 people (host + 2 guests) | One vote is not enough; when a majority votes, the video skips for everyone and the chat notes that the room voted to skip it. Votes reset on the next video |
| 3 | Reply + @mention with the other tab in the background | The reply shows the quoted message; the mentioned or replied-to person hears the two-note mention sound even though their tab is not in front (chat sounds on, and they clicked the page at least once since loading it, because browsers block sound before that) |
| 4 | Restart the server mid-video (`Ctrl+C`, then start it again within 2 minutes) | Everyone reconnects on their own; the same video keeps playing at about the right spot and the queue is intact. After more than 2 minutes it waits paused at the same spot |
| 5 | Host closes the tab | After about 10 seconds someone still in the room gets the playback controls; when the host comes back, the controls return to them |
| 6 | Paste a YouTube playlist link (with `YOUTUBE_API_KEY` set) | Offer to add the whole playlist; the videos are added to the queue up to its limit. A mix (`list=RD…`) is refused with an explanation |
| 7 | Host ticks "Keep this room forever" | The setting stays after reload; the room is not purged by `ROOM_RETENTION_DAYS` |
| 8 | Host presses the timer button ("Start together") while two devices watch | Both pause, a big 3-2-1 appears over the video on both, then the video plays on both at the same moment from the same spot. Pausing or seeking during the 3 seconds cancels it. Guests in a host-only room do not see the button |
| 9 | Send "look at 1:30" in the chat, then click the time; press the pin button next to the reactions | For the host the time is a link and the room jumps to 1:30 on every device; a guest without control sees it highlighted only. The pin posts "📌 m:ss" of the current moment, which is clickable the same way |
| 10 | Theater mode on a desktop browser, then on a phone (iPhone too) | The player fills the screen (real full screen where supported, a full-window layout on iPhone) with the latest messages, a chat box, reactions and call controls over the video; sending a message works from there; chat can be hidden; Esc or the button exits |
| 11 | Install the app: Chrome/Edge on desktop or Android via "Install app" on the landing page; iPhone via Share → Add to Home Screen | Partello opens in its own window with its icon. With the network off, reopening shows the cached app instead of the browser's offline page |
| 12 | Bell in the room header on device A (allow notifications; on iPhone/iPad from the Home Screen app, iOS 16.4+), close Partello on A, then on device B join that empty room at least 2 minutes after the server started | A gets "<room name>: B is watching … Tap to join." in the language A used; tapping it opens the room. Nothing arrives if A already has that room open in front, and only one notification per room per 10 minutes |

Scenarios 11 and 12 need HTTPS or `http://localhost`, and a production build of the frontend (`npm run build`, served by the Rust server): the service worker is not registered by the Vite dev server. To reach a phone, use the tunnel (see the end of the next section).

## Manual WebRTC test (needs real browsers & networks)

The automation in the repo tests signaling, not media. These scenarios need real devices:

| # | Scenario | Expected |
| --- | --- | --- |
| 1 | Two laptops on the same Wi-Fi, *Join call* | Microphone permission is asked at that moment (not when entering the room); audio both ways; speaking ring lights up |
| 2 | Mute/unmute | Red mic icon on the tile and in the People list; system message "muted the microphone" |
| 3 | Camera on/off during a call | Video appears/disappears without reconnecting; avatar when the camera is off |
| 4 | Deny microphone permission | Toast explains how to allow it; watching and chat still work |
| 5 | Camera in use by another app (Zoom) | Toast "being used by another app"; audio call keeps working |
| 6 | Laptop ↔ phone over mobile data | Without TURN it may fail to connect (loading icon on the tile). With `TURN_URLS` + `TURN_SHARED_SECRET` it must connect |
| 7 | Call from mobile data with Cloudflare TURN (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`, no coturn) | Startup log shows `turn=cloudflare`; the phone on mobile data connects with sound and camera both ways |
| 8 | 4–8 participants with cameras (mesh, no SFU) | Quality degrades gradually (bitrate capped); the 9th participant is refused with "call is full" |
| 9 | SFU call (`LIVEKIT_*` set) with 3+ cameras, at least one on mobile data | Startup log shows `calls=sfu`; everyone sees every camera; more than 8 people can join the call |
| 10 | Leave the call / close the tab | The device's camera/mic light goes off; other peers see the participant leave the call |
| 11 | Headset vs laptop speakers | No echo (echo cancellation on) |
| 12 | Per-participant volume (People → volume icon) | Only that person's voice changes |
| 13 | Laptop speakers (no headset) while a video plays | Friends hear you clearly; the video sound is suppressed (voice isolation) and the video gets quieter while someone speaks (auto-ducking) |
| 14 | Push-to-talk (hand icon) | Friends only hear you while the "Hold to talk" button or Space is held; typing in chat does not trigger Space |

To test across different networks without deploying, run `./scripts/tunnel.sh` (a Cloudflare quick tunnel; put TURN or LiveKit Cloud settings in `.env.tunnel`, see [DEPLOYMENT.md](DEPLOYMENT.md#6-sharing-from-your-own-computer-tunnel)). Or run Partello behind any HTTPS tunnel (e.g. `cloudflared tunnel --url http://localhost:8080`) and add the tunnel origin to `ALLOWED_ORIGINS`.

## Design gate (Taste Skill)

```bash
node <taste-skill>/scripts/lint.mjs web/src/pages web/src/index.css
node <taste-skill>/scripts/inspect.mjs http://localhost:8080/
```

Last result: lint 0 FAIL, inspect 0 FAIL. The remaining WARNs are explained in the README.
