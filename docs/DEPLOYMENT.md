# Deployment Guide

English · [Bahasa Indonesia](id/DEPLOYMENT.md)

> **Quickest route:** follow [`deploy/README.md`](../deploy/README.md). One command runs Caddy (automatic HTTPS), the app, PostgreSQL and coturn on a single VPS, with VPS sizing recommendations. This document covers the details and the other options.

Partello ships as **one Docker image** containing the Rust binary and the built frontend. Besides that you need PostgreSQL and, for reliable voice/video, a TURN server (your own coturn or a hosted one). An SFU (LiveKit) for larger calls is optional.

## 1. Build the image

```bash
docker build -t partello:latest .
```

The final image is based on `debian:bookworm-slim`, runs as a non-root user, and has a built-in `HEALTHCHECK` (`partello-server --healthcheck`).

## 2. Production configuration

| Variable | Example | Notes |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://user:pass@db:5432/partello?sslmode=require` | Migrations run automatically at startup |
| `ALLOWED_ORIGINS` | `https://partello.app` | Used for CORS **and** the WebSocket Origin check. Do not use `*` |
| `PUBLIC_BASE_URL` | `https://partello.app` | For invite links in API responses |
| `TRUST_PROXY` | `true` | Only behind a trusted proxy (rate limiting uses `X-Forwarded-For`) |
| `LOG_FORMAT` | `json` | For log aggregation |
| `STUN_URLS` / `TURN_URLS` / `TURN_SHARED_SECRET` | see below | Voice & video |
| `CLOUDFLARE_TURN_KEY_ID` / `CLOUDFLARE_TURN_API_TOKEN` | see below | Hosted TURN instead of your own coturn |
| `TURN_USERNAME` / `TURN_CREDENTIAL` | see below | Fixed TURN credentials (e.g. Metered.ca), used with `TURN_URLS` |
| `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` / `LIVEKIT_API_URL` | see below | Optional, SFU calls |
| `YOUTUBE_API_KEY` | – | Optional, for YouTube search and playlist import. Store it as a secret |
| `ROOM_RETENTION_DAYS` | `30` | Rooms untouched this long are deleted, except permanent rooms |
| `VAPID_PRIVATE_KEY` | – | Optional. Pins the push notification key (base64url P-256 private key). Empty: generated on first start and kept in the database. Store it as a secret |
| `VAPID_SUBJECT` | `mailto:ops@partello.app` | Optional contact for browser push services. Empty: `PUBLIC_BASE_URL` when it is `https://`, otherwise `mailto:push@example.org` |

All secrets are injected as environment variables or through a secret manager, **never** baked into the image or the repository.

**YouTube key.** Search and playlist import share the same `YOUTUBE_API_KEY` and its daily quota. A search costs 100 quota units (capped per day by `YOUTUBE_SEARCH_DAILY_BUDGET`, cached), a playlist import costs 2. Without a key, search is hidden and playlist links are refused with a message asking to add the videos one by one.

**Trending on the landing page.** With a key, the server fetches YouTube's most-popular chart for `TRENDING_REGION` (default `US`) once a day at `TRENDING_REFRESH_HOUR` o'clock, UTC+`TRENDING_UTC_OFFSET` (default 07:00 WIB), one call per genre (about 8 quota units). The list is stored in the database and served from there, so a restart or a deploy does not fetch it again; only a list that missed its hour (the server was down at 07:00) is refreshed at startup. A failed fetch keeps the old list and retries an hour later. Only embeddable, public, non-live, non-age-restricted videos of at least a minute are kept. Without a key, the landing page shows its built-in trailers.

**Permanent rooms.** The host can tick "Keep this room forever" in the room settings. Such a room is never deleted by `ROOM_RETENTION_DAYS`, so its link keeps working. Other rooms (and their chat) are deleted after that many days without activity.

**Push notifications.** Nothing to configure. On first start the server generates a VAPID key pair and stores it in the `server_settings` table, so it survives restarts and is included in database backups. Every browser subscription is tied to that key: if it is lost or replaced (a fresh database, a different `VAPID_PRIVATE_KEY`), existing subscriptions stop working and people have to press the bell again. Set `VAPID_PRIVATE_KEY` only to keep the same key across databases. Push needs HTTPS (see below); the startup log warns `push notifications disabled` if the key could not be loaded. The server only ever sends to the push services of the major browsers (Google, Mozilla, Apple, Microsoft), so outbound HTTPS to them must be allowed.

## 3. HTTPS and WebSocket

Put a reverse proxy (Caddy, nginx, Traefik, or a cloud load balancer) in front of port 8080:

- TLS is required: WebRTC (`getUserMedia`) only works on secure origins, and the WebSocket must be `wss://`.
- The service worker (installable app, offline page, push notifications) also needs a secure origin. `http://localhost` counts as secure for development; any other plain-HTTP address does not.
- Forward the WebSocket upgrade for `/api/rooms/*/ws` with an idle timeout of ≥ 75 seconds (the server pings every 20 seconds).

Caddy example:

```
partello.app {
  reverse_proxy app:8080
}
```

nginx example (excerpt):

```nginx
location / {
  proxy_pass http://app:8080;
  proxy_http_version 1.1;
  proxy_set_header Upgrade $http_upgrade;
  proxy_set_header Connection $connection_upgrade;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_read_timeout 120s;
}
```

## 4. TURN

STUN alone is not enough for symmetric NAT, mobile data (CGNAT) and office/campus networks. Partello can use one of three TURN sources. If several are configured, the first one in this order wins:

1. **Cloudflare Realtime TURN** (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`)
2. **Your own coturn** with a shared secret (`TURN_URLS` + `TURN_SHARED_SECRET`)
3. **Static credentials**, e.g. Metered.ca (`TURN_URLS` + `TURN_USERNAME` + `TURN_CREDENTIAL`)

The startup log line shows which one is active (`turn=cloudflare|coturn|static|none`).

### 4a. coturn

Run coturn with `use-auth-secret`:

```
listening-port=3478
tls-listening-port=5349
fingerprint
use-auth-secret
static-auth-secret=<long-random-secret>
realm=partello.app
cert=/etc/ssl/turn.pem
pkey=/etc/ssl/turn.key
```

Then set in Partello:

```
TURN_URLS=turn:turn.partello.app:3478?transport=udp,turns:turn.partello.app:5349?transport=tcp
TURN_SHARED_SECRET=<the same secret>
```

The server mints credentials valid for `TURN_CREDENTIAL_TTL_SECS` (default 6 hours) for each authenticated participant. The shared secret is never sent to the browser.

To test TURN by hand, generate credentials with the same formula:

```bash
SECRET='value of TURN_SHARED_SECRET'
USERNAME="$(( $(date +%s) + 3600 )):test"
PASSWORD=$(printf '%s' "$USERNAME" | openssl dgst -sha1 -hmac "$SECRET" -binary | base64)
echo "$USERNAME / $PASSWORD"
```

For local testing there is `docker compose --profile turn up -d coturn`.

### 4b. Cloudflare Realtime TURN (free tier, no server of your own)

Useful when Partello runs behind a tunnel or on a VPS without coturn. In the Cloudflare dashboard go to **Realtime → TURN → Create TURN key**, then set:

```
CLOUDFLARE_TURN_KEY_ID=<key id>
CLOUDFLARE_TURN_API_TOKEN=<API token>
```

No `TURN_URLS` needed. The server requests short-lived credentials from Cloudflare when someone needs them (valid 6 hours, cached and reused for 3 hours), so the API token stays on the server. If Cloudflare does not answer, the call still works for everyone STUN can connect, and a warning is logged. Check Cloudflare's current free-tier limits before relying on it.

### 4c. Static credentials (e.g. Metered.ca free plan)

```
TURN_URLS=<the TURN URLs from the provider>
TURN_USERNAME=<username>
TURN_CREDENTIAL=<credential>
```

These credentials are fixed and visible to anyone in a room (the browser receives them as they are), so prefer Cloudflare when you can. Check the provider's plan for its limits.

## 5. SFU calls with LiveKit (optional)

Without an SFU, calls are a peer-to-peer mesh: every device sends its camera to every other device, which gets heavy beyond about 8 people. When `LIVEKIT_URL`, `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET` are all set, calls go through LiveKit instead. Each device sends its camera once and LiveKit forwards it to the others.

| Variable | Notes |
| --- | --- |
| `LIVEKIT_URL` | Address browsers connect to, `wss://…` |
| `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` | Key pair LiveKit is configured with. The secret stays on the server |
| `LIVEKIT_API_URL` | Where Partello reaches LiveKit's admin API (to remove people from a call). Empty: `LIVEKIT_URL` over `https://` |
| `ROOM_MAX_VOICE` | Empty: 8 with the mesh, everyone in the room (`ROOM_MAX_PARTICIPANTS`) with an SFU |

Browsers never see the secret. After the room accepts someone into the call, the server gives that browser a short-lived token valid only for that room's call and only for publishing a camera and microphone. The LiveKit origin is added to the Content Security Policy automatically.

Two ways to run it:

1. **Bundled with the VPS stack**: `--profile sfu` in [`deploy/docker-compose.prod.yml`](../deploy/docker-compose.prod.yml). Caddy proxies `/rtc` on the same domain to LiveKit. Details, firewall ports and sizing in [`deploy/README.md`](../deploy/README.md#bigger-calls-with-an-sfu-optional).
2. **LiveKit Cloud** (free tier, also works with the tunnel): create a project, copy its `wss://` URL, API key and secret, and leave `LIVEKIT_API_URL` empty.

With an SFU, call media flows through the LiveKit server instead of directly between devices, so its bandwidth and CPU grow with the number of cameras.

## 6. Sharing from your own computer (tunnel)

`./scripts/tunnel.sh` runs Partello on your computer behind a free Cloudflare quick tunnel and prints an `https://….trycloudflare.com` address. The address changes on every run, and the computer must stay on while people watch.

Optional settings go in `.env.tunnel` (copy it from [`.env.tunnel.example`](../.env.tunnel.example); the file is git-ignored). The script loads it on start:

- `CLOUDFLARE_TURN_KEY_ID` / `CLOUDFLARE_TURN_API_TOKEN` or `TURN_URLS` / `TURN_USERNAME` / `TURN_CREDENTIAL`: a free TURN relay so calls work on mobile data.
- `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`: LiveKit Cloud for bigger calls.
- `YOUTUBE_API_KEY`: YouTube search and playlist import.

The tunnel is HTTPS, so installing the app and push notifications work through it. Because the address changes on every run, an installed app or a notification subscription from an earlier run points at an address that no longer exists; install and subscribe again on the new address.

## 7. Health checks

- `GET /healthz`: the process is alive (liveness).
- `GET /readyz`: the database is reachable (readiness); returns 503 if not.

## 8. Scaling and restarts

This version is designed for **a single instance** (room sessions live in memory). Do not run two instances against the same database. For more than one instance:

1. Use sticky routing on the path `/r/{slug}` and `/api/rooms/{slug}/…` so a room always lands on the same instance, or
2. Follow the migration path in [ARCHITECTURE.md](ARCHITECTURE.md#scale-and-distributed-state) (state and fan-out in Redis).

Each room's current video and queue are saved in PostgreSQL (`rooms.saved_state`): every 5 seconds when something changed, before an idle room is unloaded from memory, and on graceful shutdown. A restart (deploy) still drops every connection, because sessions are in memory, so everyone reconnects; that takes a few seconds. No "someone started watching" notifications are sent in the first 2 minutes after a start, so this mass reconnect does not wake everyone's phone. Clients reconnect and rejoin automatically. A video that was playing keeps playing if the server was back within 2 minutes; after a longer outage it waits paused at the same spot.

## 9. Go-live checklist

- [ ] HTTPS + `wss://` working, `ALLOWED_ORIGINS` correct
- [ ] TURN running and tested from a mobile data connection
- [ ] Push notification tested: bell on one device, start watching from another after the first 2 minutes of uptime
- [ ] If using the SFU, a call with 3+ cameras tested from mobile data
- [ ] Scheduled PostgreSQL backups
- [ ] JSON logs shipped to an observability system; alert on `/readyz`
- [ ] Rate limiting at the edge/proxy (the app's rate limit is per instance)
- [ ] WebSocket load test for the target number of concurrent rooms
- [ ] Security review (see the limitations in the README)
