# Panduan Deployment

[English](../DEPLOYMENT.md) · Bahasa Indonesia

> **Cara tercepat:** ikuti [`deploy/README.id.md`](../../deploy/README.id.md). Satu perintah menjalankan Caddy (HTTPS otomatis), aplikasi, PostgreSQL, dan coturn di satu VPS, lengkap dengan rekomendasi ukuran VPS. Dokumen ini menjelaskan detail dan opsi lainnya.

Partello dikirim sebagai **satu image Docker** yang berisi binary Rust dan hasil build frontend. Kebutuhan lainnya: PostgreSQL dan, untuk voice/video yang andal, server TURN (coturn sendiri atau layanan TURN). SFU (LiveKit) untuk call yang lebih besar bersifat opsional.

## 1. Build image

```bash
docker build -t partello:latest .
```

Image akhir berbasis `debian:bookworm-slim`, berjalan sebagai user non-root, dan memiliki `HEALTHCHECK` bawaan (`partello-server --healthcheck`).

## 2. Konfigurasi produksi

| Variabel | Contoh | Catatan |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://user:pass@db:5432/partello?sslmode=require` | Migrasi berjalan otomatis saat start |
| `ALLOWED_ORIGINS` | `https://partello.app` | Dipakai CORS **dan** cek Origin WebSocket. Jangan pakai `*` |
| `PUBLIC_BASE_URL` | `https://partello.app` | Untuk link undangan di respons API |
| `TRUST_PROXY` | `true` | Hanya jika di belakang proxy tepercaya (rate limit memakai `X-Forwarded-For`) |
| `LOG_FORMAT` | `json` | Untuk agregasi log |
| `STUN_URLS` / `TURN_URLS` / `TURN_SHARED_SECRET` | lihat bawah | Voice & video |
| `CLOUDFLARE_TURN_KEY_ID` / `CLOUDFLARE_TURN_API_TOKEN` | lihat bawah | TURN terkelola sebagai pengganti coturn sendiri |
| `TURN_USERNAME` / `TURN_CREDENTIAL` | lihat bawah | Kredensial TURN tetap (mis. Metered.ca), dipakai bersama `TURN_URLS` |
| `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` / `LIVEKIT_API_URL` | lihat bawah | Opsional, call lewat SFU |
| `YOUTUBE_API_KEY` | – | Opsional, untuk pencarian YouTube dan impor playlist. Simpan sebagai secret |
| `ROOM_RETENTION_DAYS` | `30` | Room yang tidak disentuh selama ini dihapus, kecuali room permanen |

Semua secret disuntikkan sebagai environment variable atau secret manager, **bukan** dimasukkan ke image atau repository.

**Key YouTube.** Pencarian dan impor playlist memakai `YOUTUBE_API_KEY` dan kuota harian yang sama. Satu pencarian memakan 100 unit kuota (dibatasi per hari oleh `YOUTUBE_SEARCH_DAILY_BUDGET`, dan di-cache), satu impor playlist memakan 2 unit. Tanpa key, pencarian disembunyikan dan link playlist ditolak dengan pesan agar video ditambahkan satu per satu.

**Room permanen.** Host bisa mencentang "Simpan room ini selamanya" di pengaturan room. Room seperti ini tidak pernah dihapus oleh `ROOM_RETENTION_DAYS`, jadi link-nya tetap berlaku. Room lain (beserta chat-nya) dihapus setelah tidak aktif selama jumlah hari tersebut.

## 3. HTTPS dan WebSocket

Letakkan reverse proxy (Caddy, nginx, Traefik, atau load balancer cloud) di depan port 8080:

- TLS wajib: WebRTC (`getUserMedia`) hanya bekerja di origin aman, dan WebSocket harus `wss://`.
- Teruskan upgrade WebSocket untuk `/api/rooms/*/ws` dengan timeout idle ≥ 75 detik (server mengirim ping setiap 20 detik).

Contoh Caddy:

```
partello.app {
  reverse_proxy app:8080
}
```

Contoh nginx (potongan):

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

STUN saja tidak cukup untuk NAT simetris, data seluler (CGNAT), dan jaringan kantor/kampus. Partello bisa memakai salah satu dari tiga sumber TURN. Jika lebih dari satu diisi, yang dipakai adalah yang pertama menurut urutan ini:

1. **Cloudflare Realtime TURN** (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`)
2. **coturn sendiri** dengan shared secret (`TURN_URLS` + `TURN_SHARED_SECRET`)
3. **Kredensial tetap**, mis. Metered.ca (`TURN_URLS` + `TURN_USERNAME` + `TURN_CREDENTIAL`)

Baris log saat start menunjukkan mana yang aktif (`turn=cloudflare|coturn|static|none`).

### 4a. coturn

Jalankan coturn dengan `use-auth-secret`:

```
listening-port=3478
tls-listening-port=5349
fingerprint
use-auth-secret
static-auth-secret=<secret-panjang-acak>
realm=partello.app
cert=/etc/ssl/turn.pem
pkey=/etc/ssl/turn.key
```

Lalu set di Partello:

```
TURN_URLS=turn:turn.partello.app:3478?transport=udp,turns:turn.partello.app:5349?transport=tcp
TURN_SHARED_SECRET=<secret yang sama>
```

Server mencetak kredensial yang berlaku `TURN_CREDENTIAL_TTL_SECS` (default 6 jam) untuk setiap peserta yang sudah terautentikasi. Shared secret tidak pernah dikirim ke browser.

Untuk menguji TURN secara manual, buat kredensial dengan rumus yang sama:

```bash
SECRET='isi TURN_SHARED_SECRET'
USERNAME="$(( $(date +%s) + 3600 )):test"
PASSWORD=$(printf '%s' "$USERNAME" | openssl dgst -sha1 -hmac "$SECRET" -binary | base64)
echo "$USERNAME / $PASSWORD"
```

Untuk pengujian lokal tersedia `docker compose --profile turn up -d coturn`.

### 4b. Cloudflare Realtime TURN (gratis, tanpa server sendiri)

Berguna saat Partello berjalan di balik tunnel atau di VPS tanpa coturn. Di dashboard Cloudflare buka **Realtime → TURN → Create TURN key**, lalu isi:

```
CLOUDFLARE_TURN_KEY_ID=<key id>
CLOUDFLARE_TURN_API_TOKEN=<API token>
```

`TURN_URLS` tidak perlu diisi. Server meminta kredensial berumur pendek ke Cloudflare saat dibutuhkan (berlaku 6 jam, di-cache dan dipakai ulang selama 3 jam), jadi API token tetap di server. Jika Cloudflare tidak menjawab, call tetap jalan untuk semua yang bisa tersambung lewat STUN, dan peringatan dicatat di log. Periksa batas paket gratis Cloudflare yang berlaku sebelum mengandalkannya.

### 4c. Kredensial tetap (mis. paket gratis Metered.ca)

```
TURN_URLS=<URL TURN dari penyedia>
TURN_USERNAME=<username>
TURN_CREDENTIAL=<credential>
```

Kredensial ini tetap dan bisa dilihat siapa pun di dalam room (browser menerimanya apa adanya), jadi utamakan Cloudflare jika memungkinkan. Periksa batas paket di penyedianya.

## 5. Call lewat SFU dengan LiveKit (opsional)

Tanpa SFU, call berbentuk mesh peer-to-peer: setiap perangkat mengirim kameranya ke setiap perangkat lain, dan ini menjadi berat di atas sekitar 8 orang. Jika `LIVEKIT_URL`, `LIVEKIT_API_KEY`, dan `LIVEKIT_API_SECRET` diisi semuanya, call lewat LiveKit. Setiap perangkat cukup mengirim kameranya sekali dan LiveKit meneruskannya ke yang lain.

| Variabel | Catatan |
| --- | --- |
| `LIVEKIT_URL` | Alamat yang dihubungi browser, `wss://…` |
| `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` | Pasangan key yang dipakai LiveKit. Secret tetap di server |
| `LIVEKIT_API_URL` | Alamat API admin LiveKit yang dihubungi Partello (untuk mengeluarkan orang dari call). Kosong: `LIVEKIT_URL` lewat `https://` |
| `ROOM_MAX_VOICE` | Kosong: 8 untuk mesh, semua orang di room (`ROOM_MAX_PARTICIPANTS`) dengan SFU |

Browser tidak pernah melihat secret. Setelah room menerima seseorang masuk call, server memberi browser itu token berumur pendek yang hanya berlaku untuk call di room tersebut dan hanya untuk mengirim kamera dan mikrofon. Origin LiveKit otomatis ditambahkan ke Content Security Policy.

Dua cara menjalankannya:

1. **Satu paket dengan stack VPS**: `--profile sfu` di [`deploy/docker-compose.prod.yml`](../../deploy/docker-compose.prod.yml). Caddy meneruskan `/rtc` di domain yang sama ke LiveKit. Detail, port firewall, dan ukuran VPS ada di [`deploy/README.id.md`](../../deploy/README.id.md#call-besar-dengan-sfu-opsional).
2. **LiveKit Cloud** (ada paket gratis, juga bisa dengan tunnel): buat project, salin URL `wss://`, API key, dan secret-nya, dan biarkan `LIVEKIT_API_URL` kosong.

Dengan SFU, media call mengalir lewat server LiveKit, bukan langsung antar-perangkat, jadi bandwidth dan CPU-nya bertambah seiring jumlah kamera.

## 6. Berbagi dari komputer sendiri (tunnel)

`./scripts/tunnel.sh` menjalankan Partello di komputer Anda di balik Cloudflare quick tunnel gratis dan mencetak alamat `https://….trycloudflare.com`. Alamat ini berganti setiap kali dijalankan, dan komputer harus tetap menyala selama teman menonton.

Pengaturan opsional ditaruh di `.env.tunnel` (salin dari [`.env.tunnel.example`](../../.env.tunnel.example); file ini diabaikan git). Script memuatnya saat start:

- `CLOUDFLARE_TURN_KEY_ID` / `CLOUDFLARE_TURN_API_TOKEN` atau `TURN_URLS` / `TURN_USERNAME` / `TURN_CREDENTIAL`: relay TURN gratis supaya call jalan di data seluler.
- `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`: LiveKit Cloud untuk call yang lebih besar.
- `YOUTUBE_API_KEY`: pencarian YouTube dan impor playlist.

## 7. Health check

- `GET /healthz`: proses hidup (liveness).
- `GET /readyz`: database terjangkau (readiness); mengembalikan 503 jika tidak.

## 8. Skala dan restart

Versi ini dirancang untuk **satu instance** (sesi room ada di memori). Jangan jalankan dua instance dengan database yang sama. Untuk lebih dari satu instance:

1. Gunakan sticky routing berdasarkan path `/r/{slug}` dan `/api/rooms/{slug}/…` agar satu room selalu berada di instance yang sama, atau
2. Ikuti jalur migrasi di [ARCHITECTURE.md](ARCHITECTURE.md#skala-dan-state-terdistribusi) (state & fan-out di Redis).

Video yang sedang diputar dan antrean setiap room disimpan di PostgreSQL (`rooms.saved_state`): setiap 5 detik jika ada perubahan, sebelum room idle dilepas dari memori, dan saat shutdown yang rapi (graceful). Restart (deploy) tetap memutus semua koneksi karena sesi ada di memori, sehingga semua orang tersambung ulang; ini butuh beberapa detik. Klien tersambung ulang dan bergabung otomatis. Video yang sedang diputar akan lanjut jika server kembali dalam 2 menit; jika lebih lama, video menunggu dalam keadaan pause di posisi yang sama.

## 9. Checklist sebelum go-live

- [ ] HTTPS + `wss://` aktif, `ALLOWED_ORIGINS` tepat
- [ ] TURN berjalan dan diuji dari jaringan seluler
- [ ] Jika memakai SFU, call dengan 3+ kamera sudah diuji dari data seluler
- [ ] Backup PostgreSQL terjadwal
- [ ] Log JSON dikirim ke sistem observability; alert untuk `/readyz`
- [ ] Rate limit di edge/proxy (rate limit aplikasi per instance)
- [ ] Uji beban WebSocket untuk target jumlah room bersamaan
- [ ] Review keamanan (lihat keterbatasan di README)
