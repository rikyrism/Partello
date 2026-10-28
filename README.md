<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/wordmark-dark.svg">
    <img src="docs/assets/wordmark-light.svg" alt="Partello" width="360">
  </picture>
</p>

<p align="center">
  <strong>Watch together. Wherever.</strong><br>
  Self-hostable watch parties: YouTube in perfect sync, live chat, voice and camera, and an API to open rooms from your own app.
</p>

<p align="center">
  <a href="https://github.com/rikyrism/Partello/actions/workflows/ci.yml"><img src="https://github.com/rikyrism/Partello/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-1a4be0" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/backend-Rust%20%C2%B7%20Axum-111111" alt="Rust and Axum">
  <img src="https://img.shields.io/badge/frontend-React%20%C2%B7%20TypeScript-111111" alt="React and TypeScript">
  <img src="https://img.shields.io/badge/calls-WebRTC-111111" alt="WebRTC">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#share-with-friends-for-free">Share for free</a> ·
  <a href="#deploy">Deploy</a> ·
  <a href="#developer-api">API</a> ·
  <a href="#how-it-works">How it works</a>
</p>

---

Create a room, send the link, press play. Everyone watches the same frame at the same time while you chat, talk and see each other. No accounts, no extensions, no apps to install.

## Features

**Watch in sync**
- Play, pause, seek and video changes reach everyone at once; late arrivals land on the exact second.
- The server keeps one clock per room; every player measures its drift twice a second and quietly corrects it.
- Shared queue with search, whole-playlist import (both through the YouTube Data API, optional), "play now", reorder, and auto-advance.
- Vote to skip: when the host holds the remote, the room can still skip a video once most people agree.
- Quality and subtitles stay per viewer through YouTube's own player; subtitle language follows the browser.

**Talk while you watch**
- Real-time chat with history for people who join late, replies, @mentions, notification sounds and unread badges.
- Emoji reactions that float over the video for everyone.
- Voice and camera over WebRTC: peer-to-peer for small groups, or through a [LiveKit](https://livekit.io) SFU for bigger calls. Mute, push-to-talk, per-person volume and speaking indicators.
- The video automatically quiets down while anyone talks, so voices stay clear.

**Rooms that just work**
- Unguessable 12-character room codes, an optional 4-digit PIN (Argon2-hashed, brute-force locked), host controls and kick.
- Host controls pass to someone else when the host drops out, and come back when they return.
- Rooms can be kept forever, so a group reuses the same link every week.
- One-click invitation with a ready-to-paste message that includes the link, the PIN and what's playing.
- Survives flaky mobile networks and server restarts: reconnects on its own, and the video and queue are still there.
- English and Indonesian interface, switchable on every page.

**Built to be hosted**
- A single Rust binary serves the API, WebSockets and the web app; PostgreSQL stores rooms and chat.
- Health and readiness probes, structured JSON logs, rate limiting, strict CSP, Docker images and CI.
- A public **developer API** with self-serve keys to create rooms and control what plays from your own product.

## Quick start

Run everything with Docker:

```bash
git clone https://github.com/rikyrism/Partello.git
cd Partello
docker compose up --build
```

Open <http://localhost:8080>, write your name on the ticket and create a room.

### Development

Requirements: Rust (stable), Node.js 22+, Docker.

```bash
# 1. Database
cp .env.example .env
POSTGRES_PORT=5433 docker compose up -d postgres

# 2. Backend on :8080 (runs migrations on start)
cd server
DATABASE_URL=postgres://partello:partello@localhost:5433/partello cargo run

# 3. Frontend on :5173 (proxies /api and WebSockets to :8080)
cd web
npm install
npm run dev
```

Without `DATABASE_URL` the server uses an in-memory store, which is handy for quick experiments.

| Command | Where | What it does |
| --- | --- | --- |
| `cargo test` | `server/` | Unit and end-to-end tests (two real WebSocket clients per scenario) |
| `cargo clippy --all-targets -- -D warnings` | `server/` | Lints |
| `npm test` | `web/` | Vitest + Testing Library |
| `npm run lint` · `npm run typecheck` | `web/` | ESLint and TypeScript |

## Share with friends for free

No server and no domain needed. A free [Cloudflare quick tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) gives your laptop a public HTTPS address:

```bash
brew install cloudflared   # once
./scripts/tunnel.sh        # Ctrl+C to stop
```

The script starts the database, builds the app, opens the tunnel and prints an address like `https://something.trycloudflare.com`. Create your room there and send the invitation. Your computer has to stay on and the address changes on every run.

Friends on mobile data usually need a TURN relay for voice and video. Copy `.env.tunnel.example` to `.env.tunnel` and fill in a free [Cloudflare Realtime TURN](https://developers.cloudflare.com/realtime/turn/) key (or a Metered.ca account); the same file takes a LiveKit Cloud project for bigger calls and a YouTube API key for search and playlists.

## Deploy

[`deploy/`](deploy/) contains a production stack for a single VPS: **Caddy** (automatic HTTPS), the app, **PostgreSQL** (not exposed) and **coturn** (TURN relay so calls connect across mobile and corporate networks).

```bash
cp deploy/.env.prod.example deploy/.env   # domain, public IP, two random secrets
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

YouTube streams straight from YouTube to every viewer and calls are peer-to-peer, so the server stays light: **1–2 vCPU and 2 GB RAM** is plenty to start. For calls with many cameras, add `--profile sfu` to run a LiveKit media server next to it (or point Partello at LiveKit Cloud). See [deploy/README.md](deploy/README.md) for sizing, firewall ports and backups.

## Developer API

Open Partello rooms from your website, bot or app. Create a key on `/developers` (or `POST /api/v1/keys`), then call the API from your server:

```bash
curl -X POST https://your-partello.example/api/v1/rooms \
  -H "Authorization: Bearer $PARTELLO_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "Launch party", "video_url": "https://youtu.be/dQw4w9WgXcQ", "pin": "0420"}'
```

```json
{
  "invite_url": "https://your-partello.example/r/k7Qm2xPa9Rt4",
  "host_url": "https://your-partello.example/r/k7Qm2xPa9Rt4#host=phk_…",
  "room": { "slug": "k7Qm2xPa9Rt4", "name": "Launch party", "has_password": true }
}
```

Also available: room status, play a video now, and append up to 50 videos to the queue. The full contract is in [`docs/openapi.yaml`](docs/openapi.yaml), served at `/api/openapi.yaml`.

## How it works

```
Browser (React)                               Server (one Rust process)
 ├─ YouTube IFrame player  ◀── sync ──┐        ├─ REST: rooms, join, settings, developer API
 ├─ Room store + socket    ◀── WSS ───┼──────▶ ├─ WebSocket hub: presence, playback, queue, chat
 └─ WebRTC mesh  ◀── peer-to-peer ──▶ peers    ├─ Pure room state machine (fully unit tested)
                                               └─ PostgreSQL: rooms, chat history, API keys
```

- **The server is the source of truth.** Playback is stored as `position` at `updated_at` plus `playing/paused` and a `version`; clients estimate the server clock (NTP-style) and compute where the video should be.
- **No sync loops.** Programmatic player changes are suppressed, and only clicks inside the focused player count as user intent. Seeks are detected as local jumps, so buffering is never mistaken for a seek.
- **Calls** use one deterministic offerer per pair, with audio and video transceivers created up front, so turning the camera on or off is a `replaceTrack` without renegotiation. TURN credentials are short-lived (coturn REST scheme) and the secret never reaches browsers.
- **State** for live rooms is kept in memory on one instance and saved to PostgreSQL every few seconds (and on shutdown), together with rooms and chat, so a restart picks up where everyone was.

Deeper notes: [architecture](docs/ARCHITECTURE.md), [WebSocket protocol](docs/WEBSOCKET.md), [REST API](docs/API.md), [testing](docs/TESTING.md), [deployment](docs/DEPLOYMENT.md). Also in [Bahasa Indonesia](docs/id/).

## Project structure

```
server/          Rust backend (Axum, Tokio, SQLx)
  src/room/      Room rules: playback, queue, presence, permissions (pure, no I/O)
  src/realtime/  WebSocket protocol, connection loop, room hub
  src/routes/    REST endpoints and the developer API
  migrations/    PostgreSQL migrations
  tests/         End-to-end tests with real WebSocket clients
web/             React + TypeScript + Vite + Tailwind
  src/features/  player, chat, queue, people, call (WebRTC), invite
  src/i18n/      English source strings and the Indonesian dictionary
deploy/          Production stack: Caddy, app, PostgreSQL, coturn
scripts/         tunnel.sh for sharing from a laptop
docs/            Architecture, protocol, OpenAPI, testing, deployment (docs/id: Indonesian)
```

## Limitations

- One instance per deployment (live room state is in memory and saved to the database). A restart makes everyone reconnect for a few seconds.
- Without an SFU, calls are a peer-to-peer mesh: great for small groups, heavy beyond about 8 cameras. Set up LiveKit for more.
- Video quality cannot be forced by the app; each viewer picks it in YouTube's player.
- No user accounts yet. Kicked people are blocked from rejoining for 15 minutes by address.

## Contributing

Issues and pull requests are welcome. Please run `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `npm run lint`, `npm run typecheck` and `npm test` before opening a PR. CI runs the same checks.

## License

[MIT](LICENSE)

Partello is not affiliated with YouTube. Videos play in YouTube's official embedded player and follow YouTube's Terms of Service.
