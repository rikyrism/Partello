# Putting Partello online on a single VPS

English · [Bahasa Indonesia](README.id.md)

This guide takes Partello from your laptop to the internet so friends can join from any device and network: automatic HTTPS, secure WebSocket (`wss://`), a database that is not exposed to the internet, and a TURN server so voice/video still connects on mobile data. An SFU for larger calls is optional.

## VPS size

What keeps Partello light: **YouTube video does not go through the VPS**. Every viewer fetches the video straight from YouTube; the VPS only sends small sync messages and chat. Voice/video calls are also direct between devices (peer-to-peer). The VPS only carries media when a connection **has to** go through TURN, for example a friend on mobile data (CGNAT) or on strict office/campus Wi-Fi, or when you turn on the optional SFU (see [Bigger calls with an SFU](#bigger-calls-with-an-sfu-optional)).

| Need | Spec | Good for |
| --- | --- | --- |
| **Starting out / friends** (recommended start) | **1–2 vCPU, 2 GB RAM, 25 GB SSD, ≥ 1 TB/month transfer** | A few active rooms at once, dozens of people online |
| Community | 2 vCPU, 4 GB RAM, 40–80 GB SSD, ≥ 2–3 TB/month, 1 Gbps port | Dozens of active rooms, hundreds of people online, many on camera calls |
| Bigger | Move TURN to its own VPS or a managed TURN service, use managed PostgreSQL | This version is still a single instance (sessions live in memory) |

Rough memory use: Rust app 30–80 MB, PostgreSQL 100–250 MB, Caddy 30–50 MB, coturn ~20 MB. **2 GB RAM** is chosen so building the image (compiling Rust) does not run out of memory. On a 1 GB VPS, add 2 GB of swap (see step 2) or build the image on another machine.

Estimated TURN bandwidth: in a 4-person call with cameras, one person who has to go through TURN pushes ~3 Mbps through the VPS (~1.3 GB per hour). Audio only is much smaller (~0.2 Mbps). A 1 TB/month quota is usually more than enough for personal use.

**Server location:** pick a region close to your users (low latency makes sync and calls feel instant; for Indonesia, **Jakarta** or **Singapore**). Local and global providers both work, as long as they:

- provide a public IPv4 address;
- allow UDP ports (TURN needs them);
- support Docker (Ubuntu 22.04/24.04 LTS recommended).

## What you need

1. A VPS with Ubuntu 22.04/24.04 and SSH access.
2. A **domain** (or subdomain), for example `watch.yourdomain.com`. HTTPS is required: without it, browsers refuse microphone/camera access, installing the app, and push notifications.
3. (Optional) A YouTube Data API v3 key for search and playlist import.

## Steps

### 1. Point the domain at the VPS

In your domain's DNS manager, create an **A record** `watch.yourdomain.com` → the VPS public IP. Wait until `ping watch.yourdomain.com` shows that IP.

### 2. Prepare the server

```bash
ssh root@VPS_IP

# Docker + Compose
curl -fsSL https://get.docker.com | sh

# Firewall: SSH, web, and TURN
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp
ufw allow 3478/tcp
ufw allow 3478/udp
ufw allow 49152:49352/udp
# Only with the optional SFU (--profile sfu). Do NOT open 7880.
# ufw allow 7881/tcp
# ufw allow 50100:50300/udp
ufw enable

# Only for a VPS with 1 GB RAM: 2 GB swap for the build
fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab
```

If your VPS provider has its own firewall (security group / cloud firewall), open the same ports there too.

### 3. Get the code and fill in the configuration

```bash
git clone <your-repo-URL> partello && cd partello
cp deploy/.env.prod.example deploy/.env
nano deploy/.env
```

Fill in `DOMAIN` and `PUBLIC_IP`, then generate two random secrets:

```bash
openssl rand -base64 32   # for POSTGRES_PASSWORD
openssl rand -base64 32   # for TURN_SHARED_SECRET
```

### 4. Start it

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

The first build takes a few minutes (Rust compilation). Caddy fetches the HTTPS certificate automatically on the first request.

### 5. Check it

```bash
curl https://watch.yourdomain.com/healthz   # {"status":"ok",...}
curl https://watch.yourdomain.com/readyz    # {"status":"ready"}
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env ps
```

Then test from two different devices, ideally one on **mobile data** and one on Wi-Fi: create a room, send the invite, play/pause, chat, and join the call. The full list of scenarios is in [`docs/TESTING.md`](../docs/TESTING.md).

To test TURN: open https://webrtc.github.io/samples/src/content/peerconnection/trickle-ice/ and enter `turn:watch.yourdomain.com:3478`. You can generate the username and password with the formula in [`docs/DEPLOYMENT.md`](../docs/DEPLOYMENT.md#4a-coturn). A candidate of type `relay` means TURN works.

## Bigger calls with an SFU (optional)

Without an SFU, every phone in a call sends its camera to every other phone, which gets heavy beyond about 8 cameras. With the bundled LiveKit server, each device sends its camera once to the VPS and LiveKit forwards it to the others. Calls can then hold everyone in the room (`ROOM_MAX_VOICE` defaults to `ROOM_MAX_PARTICIPANTS` instead of 8).

1. Add to `deploy/.env`:

   ```
   LIVEKIT_URL=wss://watch.yourdomain.com
   LIVEKIT_API_URL=http://host.docker.internal:7880
   LIVEKIT_API_KEY=partello                  # any name
   LIVEKIT_API_SECRET=<openssl rand -base64 32>
   ```

2. Open the SFU ports: `ufw allow 7881/tcp` and `ufw allow 50100:50300/udp` (and in the provider's firewall). **Do not** open 7880: Caddy reaches it from inside the VPS.
3. Start with the `sfu` profile:

   ```bash
   docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env --profile sfu up -d --build
   ```

Caddy proxies `/rtc` on the same domain to LiveKit, so no extra DNS record or certificate is needed. The LiveKit secret stays on the server; browsers only get a short-lived token for one call.

**Sizing with the SFU.** Call media now flows through the VPS. As a rough estimate, each published camera stream is about 0.5–1.5 Mbps coming in, and it is forwarded to each person watching it. A call of 8 people with cameras on can therefore push tens of Mbps out of the VPS. For camera-heavy calls of 8 or more, plan on at least 2 vCPU / 4 GB RAM and a plan with more bandwidth, and measure your own usage.

**No VPS for the SFU?** LiveKit Cloud has a free tier: put its `wss://` URL, API key and secret in `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`, leave `LIVEKIT_API_URL` empty, and start without `--profile sfu`.

## Maintenance

**Updating to a new version**

```bash
git pull
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

(Add `--profile sfu` if you use the bundled SFU.)

A restart drops connections briefly; everyone reconnects and rejoins on their own within a few seconds. The current video and queue of every room are saved in the database, so nothing is lost: a video that was playing keeps playing if the app was back within 2 minutes, otherwise it waits paused at the same spot.

**Database backup** (rooms, chat, saved queues, API keys, notification subscriptions and the key they are tied to), for example daily via cron:

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env exec -T postgres \
  pg_dump -U partello partello | gzip > ~/partello-$(date +%F).sql.gz
```

**Logs**

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env logs -f app
```

## So friends can join without trouble

- **Use the `https://` link from the Invite button.** A `localhost` link only opens on your own computer.
- **TURN is a must on mobile networks.** Many mobile carriers (in Indonesia, most of them) use CGNAT; without TURN, some friends can watch and chat but **cannot connect to voice/video**. This stack already includes coturn. Hosted TURN (Cloudflare Realtime TURN, Metered.ca) is for setups without your own coturn, such as the tunnel; see [`docs/DEPLOYMENT.md`](../docs/DEPLOYMENT.md#4-turn).
- **iPhone/Android:** the browser sometimes holds back video sound until the screen is tapped once; Partello shows a "Turn on sound" button.
- **Install it like an app.** On Android and desktop Chrome/Edge, the landing page shows "Install app"; on iPhone/iPad use Share → Add to Home Screen. Theater mode (the full-screen button under the video) keeps the chat, reactions and call over the video, which suits phones.
- **"Someone started watching" notifications.** The bell in the room header notifies that device when someone starts watching in the room, even with Partello closed. Nothing to configure: the server creates its notification key on first start and keeps it in the database (so restore backups rather than starting from an empty database, or everyone has to press the bell again). On iPhone/iPad it only works from the app added to the Home Screen (iOS 16.4 or later). No notifications are sent in the first 2 minutes after a restart, and at most one per room every 10 minutes.
- **Using Cloudflare (orange proxy) in front of the VPS?** Add `CLIENT_IP_HEADER: CF-Connecting-IP` to the `app` service and enable WebSocket support in Cloudflare. Without that header, every user appears as a Cloudflare IP and they can hit each other's rate limits. Note: TURN and the SFU's media ports cannot go through the Cloudflare proxy, so create a separate DNS record (e.g. `turn.yourdomain.com`, grey proxy) and use it in `TURN_URLS`.
- **One home/café network** shares one IP. The PIN attempt limit (10 per 5 minutes per IP) is loose enough for a family, but don't hand out a wrong PIN.
- **Rooms you want to keep:** tick "Keep this room forever" in the room settings. Other rooms are deleted after `ROOM_RETENTION_DAYS` (30 by default) without activity.
- **At most 16 people per room, 8 in a call.** Change these with `ROOM_MAX_PARTICIPANTS` / `ROOM_MAX_VOICE`. Without an SFU, a call is a peer-to-peer mesh, so more than 8 people on camera is heavy for phones. With the SFU, the call limit defaults to everyone in the room.
